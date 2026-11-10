//! The live transport (#183): a loopback-only TCP listener carrying the
//! same newline-delimited JSON-RPC `camcraft-cli mcp` speaks on stdio,
//! handing each line to the owning (UI) thread over a channel and
//! writing that thread's answer back to the client.
//!
//! Provenance: our own std::net + std::sync::mpsc code; the framing is
//! the MCP stdio transport's (modelcontextprotocol.io, revision
//! 2025-06-18: one JSON-RPC message per line), carried over a socket.
//! Clean-room, public sources only.
//!
//! The thread never touches a world: it only moves text. One client at
//! a time; a dropped client, a closed channel or an unreadable line is
//! logged and the listener goes on (or ends with the app), never a panic.
//! Dropping the [`Inbox`] stops and joins the thread.
//!
//! **Only JSON-RPC speaks** (review of #474): loopback is not a trust
//! boundary - any web page in the user's browser can POST to
//! 127.0.0.1. Every non-blank line must open a JSON object or array;
//! anything else (an HTTP request line, a header) closes the
//! connection before a single line of it reaches the app, so a page can
//! never smuggle a JSON-RPC body past its own HTTP headers. Reads are
//! bounded to [`MAX_LINE`] as they happen, and writes time out, so a
//! client can neither grow memory nor stall the listener.
//!
//! **An idle client lets go** (#567): one client holds the session, so
//! a client that sends no complete line for [`IDLE_TIMEOUT`] (time the
//! app spends answering does not count) is told why in an MCP
//! `notifications/message`, closed, logged by its peer address, and the
//! next client is served. A newcomer never pre-empts a connected client
//! before that: an agent pausing between calls keeps its session. The
//! deadline holds inside the read too: a client trickling bytes with no
//! newline is dropped on the same schedule as a silent one.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often a blocked wait looks at the stop flag: the bound on how
/// long dropping an [`Inbox`] can take.
const POLL: Duration = Duration::from_millis(50);

/// A line longer than this is refused and its client dropped - a client
/// that never sends a newline must not grow the app's memory forever.
const MAX_LINE: usize = 16 * 1024 * 1024;

/// How long a write to a client may block before the client is dropped -
/// a client that stops reading must not hold the listener.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// The most one read takes off the socket before the idle deadline is
/// looked at again.
const CHUNK: usize = 8 * 1024;

/// How long a client may go without sending a complete line before it
/// is dropped so the next one can be served (#567).
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How a served client's session ended, for the listener's log line.
enum End {
    /// It hung up, was refused, or the app is stopping.
    Closed,
    /// It sent no complete line for the idle window.
    Idle(Duration),
}

/// Whether a wire line may be handed to the app: JSON-RPC is always an
/// object or a batch array. An HTTP request line or header is neither.
#[must_use]
pub(crate) fn is_json_rpc_line(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_none_or(|b| *b == b'{' || *b == b'[')
}

/// The waker the listener calls after queueing a request, so an idle
/// GUI loop draws (and drains) a frame. Any thread may call it.
pub type Wake = Box<dyn Fn() + Send + 'static>;

/// One wire line from the client, waiting for the owning thread.
pub struct Request {
    line: String,
    reply: Sender<Option<String>>,
}

impl Request {
    /// An in-process request and the receiver its answer arrives on -
    /// what the listener makes per line, and what a test or an embedder
    /// hands an [`Inbox::channel`] directly.
    #[must_use]
    pub fn pair(line: impl Into<String>) -> (Self, Receiver<Option<String>>) {
        let (reply, rx) = mpsc::channel();
        (Self { line: line.into(), reply }, rx)
    }

    #[must_use]
    pub fn line(&self) -> &str {
        &self.line
    }

    /// Send the answer back (`None` for a notification). A client that
    /// went away meanwhile is logged, not an error for the caller.
    pub fn answer(self, reply: Option<String>) {
        if self.reply.send(reply).is_err() {
            tracing::info!("mcp: the client left before its answer was ready");
        }
    }
}

/// The owning thread's end: requests to answer, and (when listening on
/// a socket) the listener thread it stops and joins on drop.
pub struct Inbox {
    rx: Receiver<Request>,
    addr: Option<SocketAddr>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Inbox {
    /// Listen on 127.0.0.1:`port` (0 picks a free port; [`Inbox::addr`]
    /// says which). Loopback only by construction: no other interface
    /// is ever bound.
    ///
    /// # Errors
    /// The bind (port in use, no permission) or the thread spawn failed.
    pub fn listen(port: u16, wake: Wake) -> std::io::Result<Self> {
        Self::listen_with_idle(port, wake, IDLE_TIMEOUT)
    }

    /// [`Inbox::listen`] with its own idle window: a client sending no
    /// complete line for `idle` is dropped (a window shorter than the
    /// poll step is read as the poll step). Tests use a short one.
    ///
    /// # Errors
    /// The bind (port in use, no permission) or the thread spawn failed.
    pub fn listen_with_idle(port: u16, wake: Wake, idle: Duration) -> std::io::Result<Self> {
        let idle = idle.max(POLL);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("camcraft-mcp".into())
            .spawn(move || accept_loop(&listener, &tx, &flag, &wake, idle))?;
        tracing::info!("mcp: listening on {addr} (an idle client is dropped after {}s)", idle.as_secs());
        Ok(Self { rx, addr: Some(addr), stop, thread: Some(thread) })
    }

    /// An inbox with no socket: requests arrive through the returned
    /// sender. The in-process door the UI tests drive.
    #[must_use]
    pub fn channel() -> (Sender<Request>, Self) {
        let (tx, rx) = mpsc::channel();
        (tx, Self { rx, addr: None, stop: Arc::new(AtomicBool::new(false)), thread: None })
    }

    /// The next waiting request, never blocking.
    #[must_use]
    pub fn next(&self) -> Option<Request> {
        // Empty and disconnected alike: nothing to answer this frame.
        self.rx.try_recv().ok()
    }

    /// The bound address, when listening on a socket.
    #[must_use]
    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr
    }
}

impl Drop for Inbox {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("mcp: the listener thread ended abnormally");
        }
    }
}

/// `--mcp-port <n>` / `--mcp-port=<n>` over `CAMCRAFT_MCP_PORT`: `None`
/// when neither asks for the server (it is off by default).
///
/// # Errors
/// The flag without a value, or a value that is not a port number.
pub fn port_from(args: &[String], env: Option<&str>) -> Result<Option<u16>, String> {
    let flag = args.iter().enumerate().find_map(|(i, a)| {
        if a == "--mcp-port" {
            Some(args.get(i + 1).cloned())
        } else {
            a.strip_prefix("--mcp-port=").map(|v| Some(v.to_owned()))
        }
    });
    let text = match flag {
        Some(None) => return Err("--mcp-port needs a port number".into()),
        Some(Some(v)) => v,
        None => match env.map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => v.to_owned(),
            None => return Ok(None),
        },
    };
    text.trim()
        .parse::<u16>()
        .map(Some)
        .map_err(|_| format!("MCP port must be a number from 0 to 65535, got '{text}'"))
}

fn accept_loop(listener: &TcpListener, tx: &Sender<Request>, stop: &AtomicBool, wake: &Wake, idle: Duration) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => {
                tracing::info!("mcp: client {peer} connected");
                match serve_client(stream, tx, stop, wake, idle) {
                    Ok(End::Closed) => tracing::info!("mcp: client {peer} left"),
                    Ok(End::Idle(after)) => tracing::warn!(
                        "mcp: client {peer} dropped: idle - no complete line for {:.1}s; serving the next client",
                        after.as_secs_f64()
                    ),
                    Err(e) => tracing::warn!("mcp: client {peer} dropped: {e}"),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(POLL),
            Err(e) => {
                tracing::warn!("mcp: accept failed: {e}");
                std::thread::sleep(POLL);
            }
        }
    }
}

/// One client until it hangs up, the app stops, the app's end of the
/// channel is gone, or it sends no complete line for `idle`.
///
/// The idle deadline is enforced inside the read (#567 review): each
/// read waits at most the poll step or the time left in the window,
/// whichever is shorter, and takes at most [`CHUNK`] bytes, and the
/// deadline is checked after every chunk - so a client trickling bytes
/// with no newline cannot keep a read alive past its window.
fn serve_client(mut stream: TcpStream, tx: &Sender<Request>, stop: &AtomicBool, wake: &Wake, idle: Duration) -> std::io::Result<End> {
    // An accepted socket may inherit the listener's non-blocking mode
    // (BSD does); reads wait, but only a POLL at a time.
    stream.set_nonblocking(false)?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    // Bytes read but not yet a complete line. Between reads it never
    // holds a newline and never more than MAX_LINE bytes.
    let mut buf = Vec::new();
    let mut chunk = [0_u8; CHUNK];
    // The idle clock: reset by each complete line once it is answered,
    // so the app's own answering time never counts against the client.
    // A partial line never resets it.
    let mut active = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(End::Closed);
        }
        let quiet = active.elapsed();
        let Some(left) = idle.checked_sub(quiet).filter(|d| !d.is_zero()) else {
            let note = format!(
                r#"{{"jsonrpc":"2.0","method":"notifications/message","params":{{"level":"warning","logger":"camcraft","data":"idle for {}s with no complete message - the connection is closed"}}}}"#,
                quiet.as_secs()
            );
            // Best effort: the write times out, and the client goes either way.
            let _ = writeln!(writer, "{note}").and_then(|()| writer.flush());
            return Ok(End::Idle(quiet));
        };
        // Never wait past the idle deadline (nor a poll step, for the
        // stop flag). A zero timeout is refused by the OS API; `left`
        // is non-zero, and a sub-millisecond rest rounds up to 1 ms.
        stream.set_read_timeout(Some(left.min(POLL).max(Duration::from_millis(1))))?;
        // Bounded as it reads: never more than one byte past the cap.
        let room = (MAX_LINE + 1).saturating_sub(buf.len()).clamp(1, CHUNK);
        let got = match chunk.get_mut(..room).map(|c| stream.read(c)) {
            Some(Ok(n)) => n,
            Some(Err(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Some(Err(e)) => return Err(e),
            None => 0,
        };
        if got == 0 {
            // End of input: a last unterminated line still counts, as
            // it does on stdio.
            if !buf.is_empty() {
                let line = std::mem::take(&mut buf);
                forward(&line, tx, &mut writer, stop, wake)?;
            }
            return Ok(End::Closed);
        }
        buf.extend_from_slice(chunk.get(..got).unwrap_or_default());
        // Every complete line in what arrived, in order.
        while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
            let rest = buf.split_off(nl.saturating_add(1));
            let line = std::mem::replace(&mut buf, rest);
            if !forward(&line, tx, &mut writer, stop, wake)? {
                return Ok(End::Closed);
            }
            active = Instant::now();
        }
        if buf.len() > MAX_LINE {
            let refusal = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"message too long - the connection is closed"}}"#;
            writeln!(writer, "{refusal}")?;
            writer.flush()?;
            return Ok(End::Closed);
        }
    }
}

/// Hand one line to the owning thread and write its answer. `Ok(false)`
/// when the session cannot go on (the app is stopping or gone, or the
/// client is not speaking JSON-RPC).
fn forward(bytes: &[u8], tx: &Sender<Request>, writer: &mut TcpStream, stop: &AtomicBool, wake: &Wake) -> std::io::Result<bool> {
    if !is_json_rpc_line(bytes) {
        tracing::warn!("mcp: a client sent a line that is not JSON-RPC (an HTTP request?) - connection closed");
        let refusal = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"not a JSON-RPC message - the connection is closed"}}"#;
        let _ = writeln!(writer, "{refusal}").and_then(|()| writer.flush());
        return Ok(false);
    }
    let line = String::from_utf8_lossy(bytes).into_owned();
    if line.trim().is_empty() {
        return Ok(true);
    }
    let (request, answer) = Request::pair(line);
    if tx.send(request).is_err() {
        tracing::info!("mcp: the app closed its end; ending the session");
        return Ok(false);
    }
    wake();
    loop {
        match answer.recv_timeout(POLL) {
            Ok(Some(reply)) => {
                writeln!(writer, "{reply}")?;
                writer.flush()?;
                return Ok(true);
            }
            Ok(None) => return Ok(true),
            Err(RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    return Ok(false);
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                tracing::warn!("mcp: a request was dropped unanswered");
                return Ok(true);
            }
        }
    }
}
