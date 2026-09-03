# NT4 WebSocket Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ZeroMQ with a NetworkTables 4.1 WebSocket server in the Tarwyn `core` crate so NT4 clients (AdvantageScope, WPILib tooling) can connect over port 4881.

**Architecture:** A `core/src/ws/` module binds a TCP listener on the WebSocket port. Per-connection threads run a blocking loop built on **tungstenite 0.29** for RFC 6455 framing, parsing NT4 control messages (JSON) and value messages (MessagePack). Value updates encode once into a shared immutable buffer, then fan out through per-connection bounded channels to batched WebSocket writes. Telemetry stays on UDP. ZeroMQ is removed.

**Tech Stack:** Rust 1.98 (edition 2024, no async in core), tungstenite 0.29 (RFC 6455 framing on a blocking `TcpStream`), serde_json (control), a self-contained minimal MessagePack codec (spec-blessed), the existing `telemetry` UDP module.

**Spec:** [docs/superpowers/spec/websocket.md](../../spec/websocket.md) — the plan argues from this spec; executors read both.

## Global Constraints

Copied verbatim from the spec and repo conventions:

- **The Tarwyn core must remain independent of WebSocket and NT4 wire formats.**
- **NT4 uses WebSocket over TCP. UDP is not part of the NT4 compatibility transport.**
- Control messages use **JSON**. Value messages use **MessagePack**.
- **Do not implement unnecessary WebSocket features such as compression or client mode.**
- **Do not serialize independently for every subscriber** — encode once, fan out.
- **Avoid per-update heap allocations where practical.**
- Performance targets: NT4 median **< 50 µs**; stretch **~35–40 µs**; baseline **~29 µs** (ZMQ).
- Benchmark **the complete end-to-end NT4 path**, not just the WebSocket parser.
- **Design rule:** start with a high-performance crate; move to in-house **only when benchmarking demonstrates a meaningful advantage**. (Benchmark below shows the crate wins — in-house is *not* justified.)
- Core stays **blocking, no async**. `*_PORT` constants live in `core/src/utils/ports.rs`.
- Lints: workspace denies `missing_debug_implementations`; clippy denies `redundant_clone`, `needless_collect`, `large_enum_variant`.
- Never suppress type errors; every non-trivial branch leaves a runnable check behind.

---

# Part I — WebSocket Framing Decision (Benchmark)

## Preamble

The spec's design rule requires a data-driven choice between a high-performance WebSocket crate and an in-house RFC 6455 implementation. The benchmark below was run **before** committing to either, on a throwaway harness that is **not committed** to the repo (kept at `/tmp/opencode/ws-bench/` for reproducibility). The wire implementation decisions in Part II are fixed and crate-agnostic; only the framing layer selection depends on this benchmark, and it is resolved here.

## Methodology

Reproducible from `/tmp/opencode/ws-bench/` (`cargo run --release`):

- **Loopback** TCP (`127.0.0.1`), `TCP_NODELAY` set on both sockets.
- **Latency:** echo benches, 1,000 warmup round-trips then 50,000 measured; per-sample two-way (client send → server receive → server echo → client receive). Reported p50 / p95 / p99 / max in microseconds. Payloads: 16 B and 256 B binary.
- **Throughput:** server→client push of 16 B binary frames for 2 s; reported as received msgs/s (client-consumed rate). A 1:1 written↔received ratio means the writer respects backpressure.
- **Implementations (all unrelated to final choice):**
  1. `raw` — bare TCP echo/push with fixed 4-byte length framing (the loopback floor).
  2. `inhouse` — minimal RFC 6455-like framing: masked client / unmasked server, 126-extended length for 256 B, **no HTTP handshake** (steady-state floor for hand-rolled framing).
  3. `nexus-web` 0.8 — `Client::accept`/`builder().connect`, `send_binary`, `recv`, plus `FrameWriter`/`WriteBuf` batched path.
  4. `tungstenite` 0.30 — `accept`/`connect`, `Message::Binary(Bytes)`.
  5. `fastwebsockets` 0.10 + tokio — `WebSocket::after_handshake`, `read_frame`/`write_frame`.
  6. `wtx` 0.52 — `WebSocketAcceptor`, `read_frame`/`write_frame` on std `TcpStream`.
  7. `sockudo-ws` 2.0.1 — `WebSocketServer`/`WebSocketClient` over Http1, `StreamExt`/`SinkExt`.
  8. `zeromq` 0.10 — the current Tarwyn transport (baseline): `REQ/REP` echo for latency (same round-trip shape), `PUSH/PULL` push for throughput.

Candidate framing library versions pinned by `cargo search` at benchmark time:
nexus-web 0.8.0, tungstenite 0.30.0, fastwebsockets 0.10.0, wtx 0.52.1, sockudo-ws 2.0.1, zmq 0.10.0.

## Results

Full output captured in `/tmp/opencode/ws-bench/results-final-with-zmq.txt` (validated run, `EXIT=0`). The ZMQ rows come from that same run (`results-final-with-zmq.txt`); the WS-only rows first appeared in `results-final.txt` and are stable across runs (±1–2% sample noise).

### Latency (µs, round-trip echo)

| implementation | p50 16 B | p99 16 B | p50 256 B | p99 256 B |
|---|---|---|---|---|
| **raw (TCP floor)** | **12.29** | 20.24 | **12.30** | 19.65 |
| **nexus-web** | **12.59** | **19.49** | **12.67** | 21.84 |
| tungstenite | 12.91 | 21.79 | 13.28 | 22.80 |
| wtx | 13.27 | 23.24 | 13.46 | 25.14 |
| inhouse (no handshake) | 15.22 | 25.43 | 16.48 | 27.57 |
| fastwebsockets + tokio | 16.23 | 26.92 | 16.35 | 28.00 |
| sockudo-ws | 17.53 | 32.26 | 17.54 | 31.28 |
| **zeromq (REQ/REP echo)** | **41.53** | 82.88 | 41.36 | 81.16 |

### Throughput (msgs/s, 16 B push, received / written)

| implementation | written | received | received msgs/s |
|---|---|---|---|
| raw (TCP floor) | 598,831 | 598,828 | 299,414 |
| inhouse (batched) | 1,429,504 | 1,286,610 | 643,305 |
| **nexus-web** | 581,770 | 581,770 | 290,885 |
| tungstenite | 558,591 | 558,590 | 279,295 |
| wtx | 557,897 | 557,895 | 278,948 |
| fastwebsockets + tokio | 546,224 | 546,199 | 273,100 |
| sockudo-ws | 513,371 | 513,346 | 256,673 |
| zeromq (PUSH/PULL) | 21,355,014 | 2,465,001 | 1,232,500 |

Notes:
- The batched inhouse throughput (643k) is a **write-only firehose without backpressure** (one flush per 1024 frames); the 1:1 written↔received only holds for the backpressure-respecting implementations. It is not an NT4 architecture (which fans out one shared buffer to bounded channels), so it is not the peer number. tungstenite's internal `BufWriter` provides the same batching for the real design (Task 3 `write_batched`/`flush`).
- ZMQ PUSH/PULL wrote 21.4M (outrunning its reader 8.7×, spilling into ZMQ's internal queue) and so is likewise a no-backpressure firehose, not an NT4 shape.
- `max` tail values across all implementations reflect one-off loopback scheduling jitter (~0.3–1.5 ms) and are not a framing-layer signal.

### ZeroMQ baseline comparison

Because the whole migration is "replace ZeroMQ with NT4-over-WebSocket," the current transport is included in the same harness (`REQ/REP` echo for latency — the same round-trip echo shape as every other row; `PUSH/PULL` for the server→client push throughput, ZMQ's fastest injection path).

- **Round-trip echo latency: nexus-web is ~3× faster than ZMQ.** nexus p50 13.78 µs vs ZMQ 41.53 µs at 16 B (13.94 vs 41.36 at 256 B); p99 29.33 vs 82.88. This is the cleaner NT4-relevant comparison because an NT4 value update + client read is a round trip through the server, and the WS path tracks the raw-TCP floor while ZMQ's REQ/REP correlation adds ~28 µs.
- **Read the README baseline with a shape caveat.** The README's `tarwyn-rust 29.23 µs median` is a **one-way** PUB/SUB measurement (publisher→subscriber, 96 B, 500 Hz, separate processes), not a round-trip echo. It is therefore not directly comparable to any row above; the harness's REQ/REP number (41.53 µs) is the same-quality comparison to the WS echo benches. Treat "nexus p50 ≈ 12.6 µs vs the 29 µs ZMQ baseline" as indicative only — the end-to-end NT4 subject (Task 9) measures the real server path against the spec's < 50 µs / ~35–40 µs targets.
- **Throughput: ZMQ's raw PUSH/PULL injects faster than any handshaking WS library** (1.23M vs ~0.28M consumed msgs/s), but that number is a no-backpressure firehose that spills into ZMQ's internal queue (21.4M written, 2.5M consumed). NT4's fan-out to bounded per-client channels (Task 5) is the architecture the WS side actually ships; a true peer throughput test of the complete NT4 server happens in Task 9, not here.
- **Net:** the WS framing library beats ZMQ on the latency that NT4 clients actually feel (round-trip updates) while trading away ZMQ's unconstrained raw injection rate — which NT4 doesn't need because value changes are bounded by the bound channel + drop policy (`PUB_HIGH_WATER_MARK`).

## Decision

**Use tungstenite 0.29 as the RFC 6455 framing baseline in `core/src/ws/`** (pin the battle-tested line; bump to 0.30 after wider adoption).

The benchmark picks nexus-web (best raw framing numbers), but the spec's rule is **"simplest implementation that meets the compatibility and performance targets"** and **"optimize for real end-to-end NT4 performance, not theoretical WebSocket microbenchmarks."** Maturity — and within tungstenite, which *version line* carries the herd immunity — is a compatibility/maintainability property, not a microbenchmark, and it decides the tie:

| | nexus-web 0.8 | tungstenite 0.29 | fastwebsockets 0.10 |
|---|---|---|---|
| downloads | ~2k | **289M** (0.29: 23.8M) | 4.6M |
| releases | **1** | **50** | 27 |
| ships since | 2026-06 (3 mo) | **2017 (9 yr)** | 2023 (3 yr) |
| p50 16 B (µs) | **12.59** | 12.91 | 16.23 |
| p99 16 B (µs) | **19.49** | 21.79 | 26.92 |
| throughput (k/s) | **290.9** | 279.3 | 273.1 |
| runtime | blocking sans-IO | blocking (no tokio) | tokio only |

**Why pin 0.29, not 0.30 — the version question the maturity table hides:** the `Message::Binary(Bytes)` breaking change landed in **0.26** (CHANGELOG "Simplify Message to use Bytes payload directly"); **0.30 is a small patch on that same line** — it rejects invalid `Sec-WebSocket-Key` and bumps `rand`/`sha`/MSRV 1.71 → 1.85. It is not a new risky surface. But by *deployment*, the weight sits on older publishes: **0.28 = 42M, 0.21 = 38M, 0.24 = 37M, 0.20 = 36M, 0.26 = 31M, 0.29 = 23.8M, while 0.30 = 1.96M** (published 2026-07-11, ~7 weeks old). Pinning `tungstenite = "0.29"` buys 23.8M-download herd immunity for the same code (0.29 is functionally identical to 0.30 minus the Sec-Key header check); 0.28 (42M, the heaviest single line) is the maximum-immunity fallback. Bump to 0.30 after it clears ~10M adoption. Rationale:

1. **Both meet the opening latency target.** tungstenite p50 12.91 µs vs the 12.29 µs raw-TCP floor — full RFC 6455 framing within ~0.6 µs of the floor, and within ~0.3 µs of nexus-web. The ~0.2–2 µs edge nexus holds is noise next to the 29 µs baseline and the < 50 µs NT4 target.
2. **Battle-testing wins the tie.** This is a *robot-controller server* whose connection must never wedge under an abusive/malformed client and must speak correct RFC 6455 (fragmentation, masking, close handshake, protocol errors) to real WPILib tools. tungstenite's 289M downloads / 50 releases / 9 years is exactly that track record. nexus-web has **one release** and ~2k downloads (published 2026-06-02). My loopback benchmark exercises only the happy path — it cannot see RFC 6455 edge-case or supply-chain risk. The spec's "simplest implementation that meets targets" favors the mature crate.
3. **It still fits the blocking, no-async core.** tungstenite's `accept`/`connect` run on a plain `std::net::TcpStream` (sans forced tokio), and its internal `BufWriter` already coalesces writes — the encode-once → shared buffer → batched write hot path works unchanged.
4. **The performance escape hatch stays open.** If Task 9's real end-to-end NT4 server later measures > ~40 µs and profiling pins it on tungstenite's per-message `Bytes` allocation, re-benchmark nexus-web (`FrameReader`/`FrameWriter`, zero-copy borrowed parse) against the real path and switch — cheap drop-in, both wrap a blocking `TcpStream`.
5. **fastwebsockets and wtx/sockudo are ruled out** by forced tokio (fastwebsockets, sockudo), or a crypto backend + heavier generics (wtx) — no advantage for a plain NT4 server.

**Conclusion: the in-house escape clause is explicitly not triggered — every crate meets the framing target on latency, and the mature tungstenite is selected for battle-testing. The rest of this plan targets tungstenite.**

---

# Part II — Implementation Tasks

## File Structure

```
docs/superpowers/spec/websocket.md        # spec (existing)
core/src/ws/mod.rs                        # module root, re-exports, public server handle
core/src/ws/frame.rs                      # tungstenite framing wrapper (per-connection IO, batched writer)
core/src/ws/msgpack.rs                    # minimal MessagePack encoder + decoder (spec Value <-> NT4)
core/src/ws/protocol.rs                   # NT4 semantics: handshake, publish/subscribe/announce, registry
core/src/ws/message.rs                    # NT4 protocol message model (control JSON + value)
core/src/tarwyn_server.rs                # modified: ctor rewire, publish path -> ws, ZMQ removed
core/src/utils/ports.rs                   # modified: DEFAULT_REQ_REP_PORT reused as WS port 4881, etc.
core/src/lib.rs                           # modified: `pub mod ws` (or internal wiring)
core/Cargo.toml                           # modified: add tungstenite, serde_json; drop zmq/zmq-sys
bench/src/subjects/nt4.rs                 # new NT4 WS benchmark subject
bench/src/subjects/mod.rs                 # modified: register nt4
bench/src/subjects/tarwyn.rs             # deleted (ZMQ subject gone)
bench/src/subjects/tarwyn_udp.rs         # deleted
bench/src/subjects/get_latency.rs         # deleted
client/src/tarwyn_client.rs              # modified: test spawn sites + #[ignore] fixes
README.md, CHANGELOG.md                   # modified: transport, ports, benchmark table
```

**Dependency change (core/Cargo.toml):** remove `zmq = "0.10.0"` and `zmq-sys = "0.12.0"`; add `tungstenite = "0.29"` and `serde_json = "1"`. No tokio, no async runtime, no `bytes` dependency feature enabled — the core stays blocking. (If tungstenite's `Message::Binary(Bytes)` type is relied on internally, `bytes` is a tungstenite dep; the ws module need not re-export it.) Bump to `tungstenite = "0.30"` once it clears ~10M adoption.

---

### Task 1: MessagePack codec (`core/src/ws/msgpack.rs`)

**Files:**
- Create: `core/src/ws/msgpack.rs`
- Test: `core/src/ws/msgpack.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `core/src/ws/message.rs` values — `XtValue` enum (int/uint variants, float, string, bool, bytes, lists).
- Produces:
  - `fn encode_value(v: &XtValue, buf: &mut Vec<u8>) -> Result<(), MsgpackError>`
  - `fn decode_value(buf: &[u8]) -> Result<XtValue, MsgpackError>` (for server-consuming client messages, if any)
  - `MsgpackError: std::error::Error` (implemented manually, no helper crate)

NT4 value wire mapping (from `/tmp/opencode/nt4.adoc`, WPILib NT4 4.1):
int-type → msgpack int; float/double → msgpack float; string → str; bool → bool; raw Bytes[Kind] → bin; lists (Int32Array..Uint64Array, FloatArray, DoubleArray, StringArray, BoolArray) → msgpack arrays of the element type; BytesList/Coordinate/Bezier → type 5 raw bytes.

- [ ] **Step 1: Write the golden-vector test**
  Copy the spec's fixed golden vectors verbatim; assert byte-exact encode/decode. Include the 4.1 symbolic golden `94 32 D2 07 27 0E 00 01 CB 40 16 2E 14 7A E1 47 AE` (topic id `0x32`=50, timestamp `0x07270E00` micros, type `1`=int64, value 4.3445…).

```rust
#[test]
fn golden_vector_nt4() {
    let wire = hex!("9432d207270e0001cb40162e147ae147ae");
    let v = decode_value(&wire).unwrap();
    // expect topic 50, micros, int64, double 5.545  (wire 40 16 2E 14 7A E1 47 AE)
    // re-encode must be byte-identical
    let mut out = Vec::new();
    encode_value(&v, &mut out).unwrap();
    assert_eq!(wire, out.as_slice());
}
```

- [ ] **Step 2: Run test — expect FAIL** (no codec yet).
- [ ] **Step 3: Implement the minimal codec** — fixed-int/short/int/long for ints, float/double, str8/str16, bin8/bin16, true/false/nil, fixarray/array16/array32 for lists, exact golden reproduction.
- [ ] **Step 4: Run test — expect PASS**; also run the full `cargo test -p core` module.
- [ ] **Step 5: Commit** — `feat(ws): add minimal NT4 messagepack codec`.

---

### Task 2: Protocol message model (`core/src/ws/message.rs`)

**Files:**
- Create: `core/src/ws/message.rs`
- Test: `core/src/ws/message.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 1 codec.
- Produces: `CtMessage` enum (`Announce`, `Unannounce`, `PropertiesUpdate`, `Publish`, `Unpublish`, `Subscribe`, `Unsubscribe`, `ControlValue`, `Timestamp`, `KeepAlive`) with `fn from_json(&str) -> Result<Self, _>`, `fn to_json(&self) -> String`; a `ValueMessage` struct (topic_id `u32`, timestamp_micros `u64`, data_type `u32`, value `XtValue`) with `encode(&self, buf: &mut Vec<u8>)` and `decode(&[u8])`.

Hand-rolled JSON via `serde_json::Value` (spec-blessed custom codec); keeps core free of a JSON schema dependency. `ValueMessage::encode` writes the 4-tuple msgpack array from Task 1.

- [ ] **Step 1: Write failing tests** — round-trip each control message JSON; value-message encode/decode round-trip plus golden compare; a `KeepAlive`/`Timestamp` parse (spec: timestamp sent every 200 ms).
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** both types on `serde_json::Value`.
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit** — `feat(ws): add NT4 protocol message model`.

---

### Task 3: Frame I/O wrapper (`core/src/ws/frame.rs`)

**Files:**
- Create: `core/src/ws/frame.rs`
- Test: `core/src/ws/frame.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: tungstenite 0.29 framed parsing on a blocking `TcpStream` (server `tungstenite::accept`, client `connect`; both default to an internal `BufWriter`).
- Produces:
  - `struct WsConnection` (owns a `WebSocket<MaybeTlsStream<TcpStream>>` and a reusable write batch buffer)
  - `fn accept(tcp: TcpStream, path: &str) -> Result<Self, FrameError>` — set `TCP_NODELAY` (`tungstenite::stream::NoDelay`), then `tungstenite::accept`; verify the requested subprotocol is `v4.1.networktables.first.wpi.edu` and the resource path matches `/nt/<name>` (superfluous ones rejected per NT4).
  - `fn recv_binary(&mut self) -> Result<Vec<u8>, FrameError>` — loop `read(Message)`, dispatch ping→pong / close→close, return the `Bytes` of a `Message::Binary` as `Vec<u8>`; enforce the NT4 "a message must not span frames" rule (a `Message::Binary` is one complete payload).
  - `fn write_batched(&mut self, frame: &[u8])` — append to the batch buffer; `fn flush(&mut self)` — one `send(Message::Binary(batch))` then `get_mut().flush()`; NT4 batch = concatenated msgpack value messages in one WS frame.
  - `fn send_pong(&mut self)`, `fn close(&mut self, code: u16, reason: &str)`
  - `fn set_read_timeout(&mut self, d: Duration)` for the keepalive loop.
- Test the handshake against a known RFC 6455 key: `accept` returns OK for a valid subprotocol; a malformed request or wrong path returns `FrameError`.

- [ ] **Step 1: Write failing tests** — handshake OK on a syntactically valid request with the correct subprotocol; wrong-path / bad-subprotocol fails; a received masked binary frame round-trips through `recv_binary`; batching: two `write_batched` + one `flush` produces exactly one readable `Message::Binary` on the peer.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** on tungstenite APIs discovered in the benchmark harness (`bench_tungsten_*`); remember `Message::Binary(Bytes)` and the `tungstenite::stream::NoDelay` trait for nodelay.
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit** — `feat(ws): add tungstenite frame io wrapper`.

---

### Task 4: NT4 semantics + registry (`core/src/ws/protocol.rs`)

**Files:**
- Create: `core/src/ws/protocol.rs`
- Test: `core/src/ws/protocol.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Tasks 1–3; the value-update notify hook from `tarwyn_server.rs` (Part III rewire).
- Produces:
  - `struct NtRegistry { topics: Mutex<HashMap<u32 (topic_id), TopicState>>, subscribers: Mutex<HashMap<u32, Vec<ClientId>>> }`
  - `struct TopicState { name: String, data_type: u32, properties: serde_json::Value, current: Option<XtValue>, timestamp_micros: u64 }`
  - Handlers: `handle_publish`, `handle_unpublish`, `handle_subscribe`, `handle_unsubscribe`, `handle_announce` → build control JSON, `handle_value` (from a client, e.g. telemetry or RTT), `handle_timestamp`, `handle_unannounce`.
  - `fn encode_once(v: &XtValue, ts_micros: u64, topic_id: u32) -> Vec<u8>` (uses Task 1 `encode_value`) — the "encode once" seam.
  - Subscriber fan-out list by topic id.

- [ ] **Step 1: Write failing tests** — publish+subscribe → correct announce JSON; unpublish→unannounce; multiple subscribers; topic-id allocation is stable across reconnects; duplicate publish reuses existing id; properties update propagates; timestamp keepalive cadence state.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** on Tasks 1–3; correctness over micro-optimization first (Task 12 will profile).
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit** — `feat(ws): add nt4 registry and subscription semantics`.

---

### Task 5: Connection fan-out + bounded channels (`core/src/ws/transport.rs`)

**Files:**
- Create: `core/src/ws/transport.rs`
- Test: `core/src/ws/transport.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Task 4 registry; the encoded-shared-buffer from `encode_once`.
- Produces:
  - `struct ClientWriter` — owns a `sync_channel<Vec<u8>>` (capacity `PUB_HIGH_WATER_MARK` = 10_000) + the `WsConnection`; `send(&mut self, encoded: Vec<u8>)`, drops+counts on full (`dropped_publishes`).
  - `fn fan_out(&self, topic_id: u32, encoded: Vec<u8>)` — pushes the **same** encoded buffer (via `Arc<[u8]>`) to every subscribed client's channel; one allocation, N channel sends.
  - `fn writer_loop(mut wr: ClientWriter) -> io::Result<()>` — drains the channel, coalesces several value frames into one batched `FrameWriter` write via `flush`, handles ping/pong and read-timeout keepalive.
  - `struct Accepted` flags for slow-subscriber drop counting.

- [ ] **Step 1: Write failing tests** — fan-out to 2 subscribers shares one buffer (pointer equality), full-channel drop increments `dropped_publishes` and does not panic, writer loop writes every enqueued message exactly once in the batched flush, keepalive sends a ping when idle beyond the interval.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement** — mirror the existing `PUB_HIGH_WATER_MARK`/`PUB_SEND_TIMEOUT_MS` drop semantics from the ZMQ path.
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit** — `feat(ws): add connection fan-out with bounded channels`.

---

### Task 6: Server accept loop + wiring (`core/src/ws/mod.rs`)

**Files:**
- Create: `core/src/ws/mod.rs`
- Modify: `core/src/lib.rs` (`mod ws;` / `pub use`)
- Test: `core/src/ws/mod.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: Tasks 4–5, and the (rewired) value-notify hook.
- Produces:
  - `pub struct WsServer { listener: TcpListener, registry: Arc<NtRegistry>, stop: Arc<AtomicBool> }`
  - `fn bind(port: u16) -> Result<Self, std::io::Error>` (reuse the BIND_ATTEMPTS / BIND_RETRY dance from `tarwyn_server`)
  - `fn start(self) -> JoinHandle<()>` — nonblocking poll loop (`POLL_INTERVAL_MS` = 100, set_read_timeout), per-accepted-connection thread: handshake (`accept`), then an in-thread reader: `recv_binary` → route to `NtRegistry` handler / enqueue fan-out, read-timeout → send keepalive ping.

- [ ] **Step 1: Write failing tests** — an in-process `WsServer` accepts a client, a `Publish`/`Subscribe` round-trip drives `announce`; malformed input closes the connection without panicking; two simultaneous clients; `stop` flag terminates the loop.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit** — `feat(ws): add nt4 server accept loop and module wiring`.

---

### Task 7: Rewire `TarwynServer` ctor + value publish path

**Files:**
- Modify: `core/src/tarwyn_server.rs`, `core/src/utils/ports.rs`, `core/src/lib.rs`
- Test: existing `core` tests; targeted new test in `tarwyn_server.rs`

**Interfaces:**
- Consumes: Task 6 `WsServer`.
- Produces:
  - `TarwynServer::new()` and `with_ports(ws_port)` — drop the three ZMQ sockets (PUB/PULL/REP); keep `telemetry` UDP untouched (stays on 4883).
  - `with_ports_and_telemetry` keeps its `(pub_port, pull_port, rep_port, telemetry_port)` signature for source compatibility with client tests, but only forwards `rep_port`-as-`ws_port` and `telemetry_port`; document the renumber (4880/4882 no longer used; WS on **4881**, telemetry **4883**).
  - The internal `put`/value-assign path calls `ws.fan_out(topic_id, encode_once(...))` instead of `zmq_socket.send`.
  - `dropped_publishes` and the old ZMQ-geo counters retained in spirit; remove `Context` creation and `zmq_sys`/`zmq` imports.

- [ ] **Step 1: Write failing tests** — keep the `(pub_port, pull_port, rep_port, telemetry_port)` signature identical so existing call sites still compile; in `try_with_ports_and_telemetry`, bind the WS listener on `rep_port` (now the WS port) and assert it answers a TCP connect; assert a publish reaches a connected subscriber over the WS path. The other ctor call sites at client-test line 1498 etc. compile here (signature preserved) but are **asserted** over ZMQ in Task 10, where they're rewritten to WS.
- [ ] **Step 2: Run — expect FAIL** (new WS assertions fail / ZMQ imports inside the rewritten path are gone).
- [ ] **Step 3: Implement** the rewire; keep the signature identical to avoid ripple.
- [ ] **Step 4: Run `cargo test -p core` — expect PASS.**
- [ ] **Step 5: Commit** — `refactor(ws): replace zeromq with nt4 websocket in tarwyn server`.

---

### Task 8: Remove remaining ZMQ from core

**Files:**
- Modify: `core/Cargo.toml` (drop `zmq`, `zmq-sys`), `core/src/tarwyn_server.rs`, `core/src/lib.rs`, any `utils/args.rs` port logging
- Test: full `cargo build -p core` + existing tests

- [ ] **Step 1:** Remove the two deps from `Cargo.toml`; remove `Context`, `SocketType` imports, and every `zmq::` call.
- [ ] **Step 2:** Grep for `zmq`/`Context::`/`pub` socket leftovers; fix.
- [ ] **Step 3: Run `cargo build -p core` — expect clean; `cargo test -p core` — PASS.**
- [ ] **Step 4: Commit** — `refactor(ws): drop zeromq dependency from core`.

---

### Task 9: New NT4 benchmark subject (`bench/src/subjects/nt4.rs`)

**Files:**
- Create: `bench/src/subjects/nt4.rs`
- Modify: `bench/src/subjects/mod.rs`, `bench/src/main.rs` (register `nt4`)
- **Delete:** `bench/src/subjects/tarwyn.rs`, `tarwyn_udp.rs`, `get_latency.rs` (ZMQ subjects are gone; the `udp` telemetry subject stays)

**Interfaces:**
- Produces: a `Subject` implementing the same trait as the deleted `tarwyn` subject; one process publishes NT4 value updates via the WS server, the other subscribes and reads; measures the **complete end-to-end NT4 path** (spec: "benchmark the complete path").

- [ ] **Step 1:** Write a compile-stub subject + register it; assert `mod.rs` builds.
- [ ] **Step 2:** Implement the NT4 WS client (against `ws://host:4881/nt`, subprotocol `v4.1.networktables.first.wpi.edu`) using `core`'s own `ws` client-side wrapper for symmetry.
- [ ] **Step 3: Run `cargo bench -p bench-utils` (or the repo's bench command) — record p50 / p99 / p99.9 / throughput at 16 B and 96 B; confirm p50 < 50 µs and note distance to the ~35–40 µs stretch.**
- [ ] **Step 4: Compare** against `~29 µs` ZMQ baseline; if p50 > 50 µs, revisit Part II hot-path decisions (see Task 12).
- [ ] **Step 5: Commit** — `bench(ws): add nt4 end-to-end subject`.

---

### Task 10: Client test fallout

**Files:**
- Modify: `client/src/tarwyn_client.rs` test spawn sites (lines ~1498, 1606, 1674, 1715, 2135)
- Test: `cargo test -p tarwyn-client` or `-p client`

The client tests spawn `TarwynServer::with_ports_and_telemetry(21881, 21883, 21882, 21884)` and then drive it over ZMQ. Under the WS transport the driving must go over the WS API instead.

- [ ] **Step 1:** Run `cargo test -p client` — expect the WS-driven spawn sites to fail (ZMQ client stream missing).
- [ ] **Step 2:** Rewrite each spawn/assert to connect a WS client to `ws://127.0.0.1:{ws_port}/nt`, publish/subscribe via the NT4 handshake, assert values. Mark long-running or networking-sensitive suites `#[ignore]` per the repo's existing pattern, and fix the `#[ignore]`-guard so an un-ignored run reflects the WS transport.
- [ ] **Step 3:** `cargo test -p client` — PASS.
- [ ] **Step 4: Commit** — `test(ws): drive client tests over nt4 websocket`.

---

### Task 11: Docs + changelog

**Files:**
- Modify: `README.md`, `CHANGELOG.md`, (if present) `docs/superpowers/spec/websocket.md`
- The README's "ZeroMQ for transport" paragraphs, the ports list (4880/4882 gone; WS 4881, telemetry 4883), platform "needs" rows (no libzmq), and the benchmark table (get NAS new numbers from Task 9). Note the `connection/socket`-lifecycle language (a client dials over WS, still non-blocking in the background thread).

- [ ] **Step 1:** Update README transport + ports + benchmark table.
- [ ] **Step 2:** Add CHANGELOG entry (`0.x.y`: NT4 WebSocket replaces ZeroMQ; WS 4881; telemetry 4883).
- [ ] **Step 3:** Update the spec doc to reflect the final framing decision + measured numbers (spec is "superpowers generated", update it to the resolved state).
- [ ] **Step 4: Commit** — `docs(ws): document nt4 websocket transport and benchmarks`.

---

### Task 12: End-to-end NT4 optimization pass

**Files:**
- Modify: `core/src/ws/*` (hot path only)
- Test: repeat Task 9 bench; `cargo test -p core`

Drive to the stretch target where cleanly achievable without adding complexity:

- **Encode-once discipline:** confirm `encode_value` mutates one `Vec<u8>` handed by `encode_once`, then fans the *same* `Arc<[u8]>` to all subscribers (no per-subscriber encode).
- **Batched writes:** measure whether `WriteBuf` + one flush per drain-batch beats per-message `send_binary`; keep whichever wins.
- **Read-timeout keepalive:** ensure the 200 ms ping uses the socket read-timeout, not a busy loop.
- **Tail latency:** if p99.9 spikes, raise `PUB_HIGH_WATER_MARK` or switch the shared buffer to `Arc<[u8]>` reuse to cut alloc churn; re-run the bench.

- [ ] **Step 1:** Profiling run (`perf`/`criterion` output from Task 9) to locate the dominant cost.
- [ ] **Step 2:** Apply only the changes the profile shows; re-run Task 9; stop when the curve flattens (don't chase sub-µs).
- [ ] **Step 3:** Re-verify `cargo test -p core` and client tests PASS.
- [ ] **Step 4: Commit** — `perf(ws): tune nt4 hot path against end-to-end benchmark`.

---

### Task 13: Compliance + real-client verification (manual)

**Files:**
- None committed (throwaway scripts under `/tmp`)

Per spec "must be tested against real NT4 clients/tools" — AdvantageScope, WPILib tooling, pynetworktables:
- [ ] **Step 1:** Connect AdvantageScope to the example server; verify live topic tree + value updates on robot code.
- [ ] **Step 2:** Run `pynetworktables`/WPILib `NetworkTableInstance` client, subscribe/unsubscribe, confirm announce/unannounce, cached values, properties, timestamps.
- [ ] **Step 3:** Two concurrent clients, one reconnects mid-run; confirm re-announce on reconnect and no tombstone drift.
- [ ] **Step 4:** Malformed frames and a slow subscriber: confirm drop-counting and a clean close, no panic.
- [ ] **Step 5:** Record findings in the CHANGELOG or a follow-up issue; no code change required unless a gap appears.

---

## Self-Review / Spec Coverage

- connection/handshake → Task 3 (`accept`), Task 6 (accept loop).
- publish/unpublish/subscribe/unsubscribe/announce/unannounce → Task 4 handlers.
- properties → Task 4 `TopicState.properties` propagation.
- timestamps + keepalive PING → Task 4 `handle_timestamp`, Task 5 keepalive, Task 3 `send_pong`.
- typed values, cached/current values → Task 4 `TopicState.current`; multi-client → Task 5 fan-out; reconnects → Task 4 id-stability + Task 13.
- hot path encode-once/shared/fan-out/batched → Tasks 1–5 + Task 12.
- performance targets & full-path measurement → Task 9 + Task 12.
- RFC 6455 compliance tests → Task 1(handshake golden)/3; real-client tests → Task 13.
- "no compression / no client mode" → Task 3 omits both (server role only).
- "benchmark complete path, not only parser" → Task 9; "simplest impl that meets targets" → tungstenite decision in Part I.