import argparse
import os
import sys
import time

import ntcore
from harness import HEADER_LEN, Pacer, Samples, decode, encode, now_nanos

TOPIC = "/bench/payload"
PERIODIC_SECONDS = 0.001


def deadline_secs() -> int:
    return int(os.environ.get("BENCH_DEADLINE_SECS", "120"))


def options() -> list[ntcore.PubSubOptions]:
    return [
        ntcore.PubSubOptions(
            send_all=True,
            keep_duplicates=True,
            periodic=PERIODIC_SECONDS,
            poll_storage=1000,
        )
    ]


def config_description() -> str:
    return (
        f"send_all(True), keep_duplicates(True), periodic({PERIODIC_SECONDS}s), "
        "poll_storage(1000), flush() after every set, read via read_queue()"
    )


def publish(host: str, port: int, payload: int, rate_hz: int, count: int) -> int:
    size = max(payload, HEADER_LEN)
    inst = ntcore.NetworkTableInstance.create()
    inst.start_client("bench-publisher")
    inst.set_server(host, port)

    publisher = inst.get_raw_topic(TOPIC).publish("raw", *options())
    deadline = time.time() + 10
    while not inst.is_connected() and time.time() < deadline:
        time.sleep(0.02)
    if not inst.is_connected():
        print(f"never connected to the NT server at {host}:{port}", file=sys.stderr)
        return 1

    pacer = Pacer(rate_hz)
    for seq in range(count):
        due = pacer.wait()
        publisher.set(encode(size, seq, due))
        inst.flush()
    print(f"sent {count} messages of {size} B")
    publisher.close()
    inst.stop_client()
    return 0


def serve(port: int) -> int:
    inst = ntcore.NetworkTableInstance.create()
    inst.start_server("", "", "", port)
    print(f"NT4 server on port {port}")
    sys.stdout.flush()
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        inst.stop_server()
    return 0


def subscribe(host: str, port: int, payload: int, samples: int) -> int:
    inst = ntcore.NetworkTableInstance.create()
    inst.start_client("bench-subscriber")
    inst.set_server(host, port)

    subscriber = inst.get_raw_topic(TOPIC).subscribe("raw", b"", *options())
    deadline = time.time() + 10
    while not inst.is_connected() and time.time() < deadline:
        time.sleep(0.02)
    if not inst.is_connected():
        print(f"never connected to the NT server at {host}:{port}", file=sys.stderr)
        return 1

    collected = Samples(samples)
    print(f"subscribed on {host}:{port}, waiting for {samples} samples...")
    print(f"config       {config_description()}")
    sys.stdout.flush()

    deadline = time.time() + deadline_secs()
    while not collected.full() and time.time() < deadline:
        updates = subscriber.read_queue()
        if not updates:
            continue
        received = now_nanos()
        for update in updates:
            sample = decode(update.value)
            if sample is not None:
                collected.record(sample[0], sample[1], received)

    collected.emit()
    print(f"version      {ntcore.__version__}", file=sys.stderr)
    subscriber.close()
    inst.stop_client()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)

    pub = sub.add_parser("publisher")
    pub.add_argument("--host", default="127.0.0.1")
    pub.add_argument("--port", type=int, required=True)
    pub.add_argument("--payload", type=int, default=16)
    pub.add_argument("--rate", type=int, default=500)
    pub.add_argument("--count", type=int, default=12000)

    srv = sub.add_parser("server")
    srv.add_argument("--port", type=int, required=True)

    rec = sub.add_parser("subscriber")
    rec.add_argument("--host", default="127.0.0.1")
    rec.add_argument("--port", type=int, required=True)
    rec.add_argument("--payload", type=int, default=16)
    rec.add_argument("--samples", type=int, default=3500)

    args = parser.parse_args()
    if args.command == "server":
        return serve(args.port)
    if args.command == "publisher":
        return publish(args.host, args.port, args.payload, args.rate, args.count)
    return subscribe(args.host, args.port, args.payload, args.samples)


if __name__ == "__main__":
    sys.exit(main())
