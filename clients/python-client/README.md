# tarwyn Python client

PyO3 bindings over `tarwyn_client`. Unlike the Java client, which goes through
the [C ABI](../ffi) and FFM, Python links the Rust library directly — PyO3's
per-call overhead is tens of nanoseconds rather than the ~300ns a native-to-Java
upcall costs, so there is no need for a shared-memory ring here.

    cargo build --release -p tarwyn_python
    cp target/release/libtarwyn.so tarwyn.so     # or use maturin to build a wheel

```python
import tarwyn

client = tarwyn.TarwynClient(host="10.4.88.2")
client.put_double("target_pose", 1.5)
client.put_string_list("paths", ["left", "right"])

client.start()
with client.subscribe("BEZIER_PATH", depth=64) as subscription:
    for payload in subscription.drain():
        ...
```

Construction does not block: ZeroMQ dials in the background, so a client can be
built before the server exists. `get` returns `None` rather than waiting when
there is no value, publishing discards rather than stalling when the send queue
is full, and `dropped_publishes()` reports how much was discarded.

`subscribe` buffers up to `depth` values and `drain()` empties the buffer, so a
slow consumer loses the oldest values rather than growing without bound. The
subscription is a context manager; leaving the block unsubscribes.

Every call that touches the network releases the GIL, so publishing from one
Python thread does not block others.

The list types map to native Python: `put_string_list` takes a list of `str`,
`put_bytes_list` a list of `bytes`, and the corresponding gets return the same.
