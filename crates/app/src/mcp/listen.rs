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

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// How often a blocked wait looks at the stop flag: the bound on how
/// long dropping an [`Inbox`] can take.
const POLL: Duration = Duration::from_millis(50);

/// A line longer than this is refused and its client dropped - a client
/// that never sends a newline must not grow the app's memory forever.
const MAX_LINE: usize = 16 * 1024 * 1024;

/// How long a write to a client may block before the client is dropped -
/// a client that stops reading must not hold the listener.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

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
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("camcraft-mcp".into())
            .spawn(move || accept_loop(&listener, &tx, &flag, &wake))?;
        tracing::info!("mcp: listening on {addr}");
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

fn accept_loop(listener: &TcpListener, tx: &Sender<Request>, stop: &AtomicBool, wake: &Wake) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => {
                tracing::info!("mcp: client {peer} connected");
                match serve_client(stream, tx, stop, wake) {
                    Ok(()) => tracing::info!("mcp: client {peer} left"),
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

/// One client until it hangs up, the app stops, or the app's end of
/// the channel is gone.
fn serve_client(stream: TcpStream, tx: &Sender<Request>, stop: &AtomicBool, wake: &Wake) -> std::io::Result<()> {
    // An accepted socket may inherit the listener's non-blocking mode
    // (BSD does); reads wait, but only a POLL at a time.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        // Bounded as it reads: never more than one byte past the cap.
        let room = (MAX_LINE + 1).saturating_sub(buf.len()) as u64;
        match Read::by_ref(&mut reader).take(room).read_until(b'\n', &mut buf) {
            Ok(0) => {
                // End of input: a last unterminated line still counts,
                // as it does on stdio.
                if !buf.is_empty() {
                    let line = std::mem::take(&mut buf);
                    forward(&line, tx, &mut writer, stop, wake)?;
                }
                return Ok(());
            }
            Ok(_) if buf.ends_with(b"\n") => {
                let line = std::mem::take(&mut buf);
                if !forward(&line, tx, &mut writer, stop, wake)? {
                    return Ok(());
                }
            }
            // Bytes without a newline: the next read returns 0 (EOF).
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
        if buf.len() > MAX_LINE {
            let refusal = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"message too long - the connection is closed"}}"#;
            writeln!(writer, "{refusal}")?;
            writer.flush()?;
            return Ok(());
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
