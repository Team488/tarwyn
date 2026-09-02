/// Default WebSocket port, serving reads, publishes and the control plane.
pub const DEFAULT_WS_PORT: u16 = 4881;
/// Default REQ/REP port, serving reads and the control plane.
///
/// Inert: the WS port (4881) now carries this traffic. Kept so `args.rs` and
/// `with_ports` defaults keep compiling; full cleanup is Task 8.
pub const DEFAULT_REQ_REP_PORT: u16 = 4881;
/// Default PUB/SUB port, fanning values out to subscribers.
///
/// Inert: no longer used (WS carries fan-out). Kept for source compatibility.
pub const DEFAULT_PUB_SUB_PORT: u16 = 4880;
/// Default PUSH/PULL port, receiving publishes.
///
/// Inert: no longer used (WS carries publishes). Kept for source compatibility.
pub const DEFAULT_PUSH_PULL_PORT: u16 = 4882;
