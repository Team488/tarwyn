/// Default WebSocket port, serving reads, publishes and the control plane.
///
/// NT4 4.1 fixes the unsecure standard server port at 5810, and NetworkTables
/// tools connect there, so the NT4 endpoint uses it.
pub const DEFAULT_WEBSOCKET_PORT: u16 = 5810;
/// Default REQ/REP port, serving reads and the control plane.
///
/// Inert: the WebSocket port now carries this traffic. Kept so `args.rs` and
/// `with_ports` defaults keep compiling.
pub const DEFAULT_REQ_REP_PORT: u16 = DEFAULT_WEBSOCKET_PORT;
/// Default PUB/SUB port, fanning values out to subscribers.
///
/// Inert: no longer used (WebSocket carries fan-out). Kept for source compatibility.
pub const DEFAULT_PUB_SUB_PORT: u16 = 4880;
/// Default PUSH/PULL port, receiving publishes.
///
/// Inert: no longer used (WebSocket carries publishes). Kept for source compatibility.
pub const DEFAULT_PUSH_PULL_PORT: u16 = 4882;
