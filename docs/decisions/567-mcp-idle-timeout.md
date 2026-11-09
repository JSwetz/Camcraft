## #567 - An idle live-session MCP client is dropped after a timeout

**Decided:**
- The live listener (`crates/app/src/mcp/listen.rs`) drops a client that sends no complete line for `IDLE_TIMEOUT` (5 minutes). Before closing, it sends one best-effort `notifications/message` warning that says why. The accept loop logs `client <peer> dropped: idle …` and serves the next client.
- The idle clock restarts after each answered line. Time the app spends answering doesn't count, and a partial line doesn't reset the clock.
- Windows shorter than the 50 ms poll step are raised to it, so a zero window can't drop every client at once.

**Why:** the listener serves one client at a time. Before this change, a silent local connection held the session until it hung up, and no real agent could connect meanwhile. Since #474, non-JSON traffic (browsers, TLS hellos) is refused at once, so this only concerns a silent or slow local client.

**Considered and dropped:**
- **A new client replacing an idle one (the issue's optional part).** It would let any local process kick a live agent off while that agent pauses between calls. The timeout alone frees the session.
- **A shorter default (e.g. 30 s).** An agent can legitimately think for minutes between calls; 5 minutes keeps those sessions.

**Cost to change later:** small. The window is one constant, and `Inbox::listen_with_idle` takes any duration. Replacement could be added behind an opt-in flag.
