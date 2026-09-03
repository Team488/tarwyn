/// Default REQ/REP port, used by `get` and the control plane.
///
/// The NT4 standard unsecure server port; the server's WebSocket endpoint.
pub const DEFAULT_REQ_REP_PORT: u16 = 5810;
/// Default PUB/SUB port, used by subscriptions.
pub const DEFAULT_PUB_SUB_PORT: u16 = 4880;
/// Default PUSH/PULL port, used by publishes.
pub const DEFAULT_PUSH_PULL_PORT: u16 = 4882;
