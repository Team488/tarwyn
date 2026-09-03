# v0.2.0
- NT4 WebSocket replaces ZeroMQ for the server and Rust client transport (WS 5810, endpoint `/nt/<client name>`; telemetry stays UDP 4883)
- Serves the NT4 4.1 standard port so NetworkTables tools such as AdvantageScope connect unchanged; the 4.0 subprotocol is accepted as a fallback
- Values and control now travel over a single tungstenite WebSocket connection; control uses binary protobuf Request/Reply
- ZMQ removed from core and client dependencies; no libzmq needed
- Existing `TarwynServer::with_ports_and_telemetry` signature preserved (pub/pull slots inert for source compatibility)

# v0.0.1
Initial release of tarwyn rust!
- Protobuf serialization
- Rust client & server
- Supports string & long values
- Added subscribe function
- Added get function to directly retrieve the latest value for a channel in the server

# v0.0.2
- Supports all protobuf primitives serialization
- Moved tarwyn client to a separate package
- Builds a executable file for release

# v0.1.0
Breaking:
- `subscribe_telemetry` returns a cancel handle (`Option<impl FnOnce()>`) instead
  of a bool, so a telemetry subscription can be cancelled rather than leaking its
  listener. `None` still means the registration was refused
- Subscription callbacks must now be `Send + Sync`, since the receive loop clones
  them out of the listener map before dispatching

Fixed:
- `stop()` now stops. Both receive loops blocked forever on a socket with no
  timeout, so the stop flag was only read once the next message arrived, and that
  message was served
- Receive threads are joined rather than abandoned, so restarting a client or
  server no longer accumulates them
- The UDP telemetry plane went silent for good after a stop/start cycle; the
  receiver is spawned again on `start()`
- Callbacks are dispatched with no lock held, so a callback may subscribe or
  unsubscribe without deadlocking the receive thread
- `xt_unsubscribe` cancels a telemetry subscription instead of reporting failure,
  which Java raised as an exception out of `close()`
- `stop()` before the first `start()` no longer wedges a client permanently
- The server's telemetry port is configurable via `with_ports_and_telemetry`,
  which is what a second server on one host needs
- The release archive was always empty: the packaging script copied
  `tarwyn-server` while cargo builds `tarwyn_server`, and swallowed the failure
- The logger no longer panics when installed without parsed arguments
- Prefix-matched topics no longer leave an empty entry in the listener map
- Python holds the GIL across none of the three subscribe calls, and its
  subscription registry retains the callback so its address cannot be recycled
- Java `subscribe` claims its consumer slot atomically
