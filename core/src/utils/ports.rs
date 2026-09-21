/// Default WebSocket port, serving reads, publishes and the control plane.
///
/// NT4 4.1 fixes the unsecure standard server port at 5810, and NetworkTables
/// tools connect there, so the NT4 endpoint uses it.
pub const DEFAULT_WEBSOCKET_PORT: u16 = 5810;
