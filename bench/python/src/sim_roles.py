"""The `bench sim` robot and coprocessor roles on pyntcore, for ntcore 2026 and 2027.

The robot role is the NT4 server, as on a robot, and publishes its topics from the same
process. It keeps serving for `LINGER` seconds after its window, so clients whose windows
close a little later still have a server. The coprocessor role is one NT4 client per
camera, flushed after every frame as PhotonVision does. Both print the `SIM` and `PUB`
lines `bench sim` reads, in the format of `bench/src/sim/measure.rs`.
"""

import argparse
import heapq
import json
import struct
import sys
import threading
import time

import ntcore

from ntcompat import EVENT_VALUE_ALL, call, options, start_server

HEADER_LEN = 16
FRAMING = 3
BINARY_HEADER = HEADER_LEN + FRAMING
TEXT_HEADER = 2 * HEADER_LEN + FRAMING
SCALARS = {"boolean", "double", "int", "float", "boolean[]", "double[]", "int[]", "float[]"}
TEXT = {"string", "json"}
LINGER = 2.0


def now_nanos() -> int:
    """Wall-clock nanoseconds, the clock every role stamps against."""
    return time.clock_gettime_ns(time.CLOCK_REALTIME)


def body(type_str: str) -> str:
    """How a type string's value is laid out, as `bench/src/sim/values.rs` names it."""
    if type_str in SCALARS or type_str == "string[]":
        return type_str
    if type_str in TEXT:
        return "text"
    return "binary"


def has_header(type_str: str, size: int) -> bool:
    """Whether a value of this type and captured size can carry the header."""
    kind = body(type_str)
    if kind == "binary":
        return size >= BINARY_HEADER
    if kind == "text":
        return size >= TEXT_HEADER
    return False


def size_for(spec: dict, seq: int) -> int:
    """The captured size of the `seq`th publish, cycling through the range."""
    low, high = spec["bytes"]
    return low + seq % (high - low + 1)


def make(spec: dict, seq: int, due: int):
    """The `seq`th value of a topic as `(setter, value)`, as `values::make` builds it."""
    size = size_for(spec, seq)
    length = max(size - FRAMING, 0)
    header = has_header(spec["type"], size)
    kind = body(spec["type"])

    def count(each: int) -> int:
        return max(length // each, 1)

    if kind == "binary":
        data = bytearray(length)
        if header:
            struct.pack_into("<QQ", data, 0, seq, due)
        elif data:
            data[0] = seq % 256
        return "set_raw", bytes(data)
    if kind == "text":
        text = f"{seq:016x}{due:016x}" if header else f"{seq:x}"
        return "set_string", text.ljust(length, "x")
    if kind == "boolean":
        return "set_boolean", seq % 2 == 1
    if kind == "double":
        return "set_double", seq * 0.001
    if kind == "int":
        return "set_integer", seq
    if kind == "float":
        return "set_float", float(seq)
    if kind == "boolean[]":
        return "set_boolean_array", [(seq + i) % 2 == 1 for i in range(count(1))]
    if kind == "double[]":
        return "set_double_array", [float(seq + i) for i in range(count(9))]
    if kind == "int[]":
        return "set_integer_array", [seq + i for i in range(count(9))]
    if kind == "float[]":
        return "set_float_array", [float(seq + i) for i in range(count(5))]
    return "set_string_array", [f"{seq:016x}".ljust(max(length - FRAMING, 16), "x")]


def read_header(value) -> tuple[int, int] | None:
    """The sequence number and due time a received value carries, if any."""
    if isinstance(value, (bytes, bytearray)) and len(value) >= HEADER_LEN:
        return struct.unpack_from("<QQ", value, 0)
    if isinstance(value, str) and len(value) >= 2 * HEADER_LEN:
        try:
            return int(value[:16], 16), int(value[16:32], 16)
        except ValueError:
            return None
    return None


class Fold:
    """The latencies one receiver saw for one class, and the gaps between them."""

    def __init__(self) -> None:
        self.latencies: list[int] = []
        self.lost = 0
        self.last: dict[str, int] = {}

    def record(self, topic: str, seq: int, latency_ns: int) -> None:
        self.latencies.append(max(latency_ns, 1))
        last = self.last.get(topic)
        if last is not None and seq > last + 1:
            self.lost += seq - last - 1
        self.last[topic] = seq if last is None else max(last, seq)

    def line(self, receiver: str, cls: str, expected: float, allow_us: float) -> str:
        ordered = sorted(self.latencies)

        def at(quantile: float) -> float:
            if not ordered:
                return 0.0
            return ordered[min(len(ordered) - 1, int(quantile * len(ordered)))] / 1000.0

        top = ordered[-1] / 1000.0 if ordered else 0.0
        return (
            f"SIM\t{receiver}\t{cls}\tall\t{len(ordered)}\t{expected:.1f}\t{self.lost}"
            f"\t{at(0.5):.2f}\t{at(0.99):.2f}\t{top:.2f}\t{allow_us:.1f}"
        )


class Window:
    """When a role's recording window opens and closes, on the monotonic clock."""

    def __init__(self, warmup: float, seconds: float, opens_at_ms: int | None) -> None:
        wait = 0.0 if opens_at_ms is None else max(0.0, opens_at_ms / 1000 - time.time())
        self.opens = time.monotonic() + wait + warmup
        self.closes = self.opens + seconds

    def records(self, at: float) -> bool:
        return self.opens <= at < self.closes


def schedule(publishes: list[dict], multiplier: int, start: float):
    """Yield `(due, indices)`: everything once at `start`, then each topic at its rate,
    with topics due at the same instant together."""
    yield start, list(range(len(publishes)))
    heap = []
    for index, spec in enumerate(publishes):
        if spec["rate_hz"] > 0:
            period = 1.0 / (spec["rate_hz"] * multiplier)
            heap.append((start + period, index, period))
    heapq.heapify(heap)
    while heap:
        due = heap[0][0]
        batch = []
        while heap and heap[0][0] == due:
            _, index, period = heapq.heappop(heap)
            batch.append((index, period))
        for index, period in batch:
            heapq.heappush(heap, (due + period, index, period))
        yield due, [index for index, _ in batch]


def publish(inst, client: dict, args, window: Window, flush: bool) -> list[str]:
    """Publish `client`'s topics on schedule until the window closes, and return its
    `PUB` lines.

    Publishers send every value, as AdvantageKit's do. After each batch the instance
    flushes to the network when `flush` is set, as PhotonVision does per frame, and
    otherwise only to local listeners, as WPILib's robot loop does.
    """
    publishers = []
    for spec in client.get("publishes", []):
        topic = call(inst, "get_topic", spec["topic"])
        publishers.append(
            call(
                topic,
                "generic_publish",
                spec["type"],
                options(keep_duplicates=True, send_all=True),
            )
        )
    seq = [0] * len(publishers)
    sent: dict[str, int] = {}
    specs = client.get("publishes", [])
    for due, batch in schedule(specs, args.multiplier, time.monotonic()):
        if time.monotonic() >= window.closes or due >= window.closes:
            break
        wait = due - time.monotonic()
        if wait > 0:
            time.sleep(wait)
        overshoot = max(0, int((time.monotonic() - due) * 1e9))
        due_ns = now_nanos() - overshoot
        for index in batch:
            spec = specs[index]
            seq[index] += 1
            setter, value = make(spec, seq[index], due_ns)
            call(publishers[index], setter, value)
            if window.records(time.monotonic()) and spec["rate_hz"] > 0:
                sent[spec["class"]] = sent.get(spec["class"], 0) + 1
        call(inst, "flush" if flush else "flush_local")
    asked: dict[str, float] = {}
    for spec in specs:
        if spec["rate_hz"] > 0:
            asked[spec["class"]] = (
                asked.get(spec["class"], 0.0) + spec["rate_hz"] * args.multiplier * args.seconds
            )
    return [
        f"PUB\t{client['name']}\t{cls}\t{total:.0f}\t{sent.get(cls, 0)}"
        for cls, total in sorted(asked.items())
    ]


def topics_of(profile: dict) -> dict:
    """Every published topic's spec, by name."""
    return {
        spec["topic"]: spec for client in profile["clients"] for spec in client.get("publishes", [])
    }


def robot(args, profile: dict) -> int:
    """Serve NT4, publish the robot's topics and receive its subscriptions."""
    spec = next(client for client in profile["clients"] if client["role"] == "robot")
    topics = topics_of(profile)
    window = Window(args.warmup, args.seconds, args.opens_at)
    inst = ntcore.NetworkTableInstance.create()
    start_server(inst, args.port)
    receivers = []
    for name in [name for sub in spec.get("subscribes", []) for name in sub["topics"]]:
        if name not in topics:
            continue
        topic = call(inst, "get_topic", name)
        subscriber = call(
            topic,
            "generic_subscribe",
            topics[name]["type"],
            options(send_all=True, poll_storage=1000),
        )
        receivers.append((name, topics[name], subscriber))
    folds: dict[str, Fold] = {}
    lock = threading.Lock()

    def listen(name: str, topic: dict):
        def on_value(event) -> None:
            received = now_nanos()
            if not window.records(time.monotonic()):
                return
            header = read_header(event.data.value.value())
            if header is not None:
                seq, due = header
                with lock:
                    folds.setdefault(topic["class"], Fold()).record(name, seq, received - due)

        return on_value

    for name, topic, subscriber in receivers:
        call(inst, "add_listener", subscriber, EVENT_VALUE_ALL, listen(name, topic))
    lines = publish(inst, spec, args, window, flush=False)
    for line in lines:
        print(line)
    for cls in ("camera", "struct", "slow"):
        rates = [t["rate_hz"] for _, t, _ in receivers if t["class"] == cls]
        expected = sum(rate * args.multiplier * args.seconds for rate in rates)
        if expected > 0:
            fold = folds.get(cls, Fold())
            allow = 1e6 / (max(rates) * args.multiplier)
            print(fold.line("robot", cls, expected, allow))
    sys.stdout.flush()
    time.sleep(LINGER)
    call(inst, "stop_server")
    return 0


def coprocessors(args, profile: dict) -> int:
    """One NT4 client per coprocessor, each publishing on its own thread."""
    window = Window(args.warmup, args.seconds, args.opens_at)
    cameras = [client for client in profile["clients"] if client["role"] == "coprocessor"]
    results: list[list[str]] = [[] for _ in cameras]
    instances = []
    for camera in cameras:
        inst = ntcore.NetworkTableInstance.create()
        call(inst, "start_client", camera["name"])
        call(inst, "set_server", args.host, args.port)
        instances.append(inst)
    deadline = time.monotonic() + 10
    while not all(call(inst, "is_connected") for inst in instances):
        if time.monotonic() > deadline:
            print(f"never connected to {args.host}:{args.port}", file=sys.stderr)
            return 1
        time.sleep(0.02)

    def run(index: int) -> None:
        results[index] = publish(instances[index], cameras[index], args, window, flush=True)

    threads = [threading.Thread(target=run, args=(index,)) for index in range(len(cameras))]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    for lines in results:
        for line in lines:
            print(line)
    sys.stdout.flush()
    for inst in instances:
        call(inst, "stop_client")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("role", choices=["robot", "coprocessors"])
    parser.add_argument("--profile", required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=5810)
    parser.add_argument("--multiplier", type=int, default=1)
    parser.add_argument("--warmup", type=float, default=5.0)
    parser.add_argument("--seconds", type=float, default=30.0)
    parser.add_argument("--opens-at", type=int, default=None)
    args = parser.parse_args()
    with open(args.profile) as file:
        profile = json.load(file)
    print(f"pyntcore {ntcore.__version__}", file=sys.stderr)
    if args.role == "robot":
        return robot(args, profile)
    return coprocessors(args, profile)


if __name__ == "__main__":
    sys.exit(main())
