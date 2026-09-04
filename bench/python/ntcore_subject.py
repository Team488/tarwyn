import argparse
import sys
import time

import ntcore

from harness import HEADER_LEN, Pacer, Recorder, decode, encode

TOPIC = "/bench/payload"
PERIODIC_SECONDS = 0.001


def options():
    return [
        ntcore.PubSubOptions(
            sendAll=True,
            keepDuplicates=True,
            periodic=PERIODIC_SECONDS,
            pollStorage=1000,
        )
    ]


def config_description():
    return (
        f"sendAll(True), keepDuplicates(True), periodic({PERIODIC_SECONDS}s), "
        "pollStorage(1000), flush() after every set, read via readQueue()"
    )


def publish(host, port, payload, rate_hz, count):
    size = max(payload, HEADER_LEN)
    inst = ntcore.NetworkTableInstance.create()
    inst.startClient4("bench-publisher")
    inst.setServer(host, port)

    publisher = inst.getRawTopic(TOPIC).publish("raw", *options())
    deadline = time.time() + 10
    while not inst.isConnected() and time.time() < deadline:
        time.sleep(0.02)
    if not inst.isConnected():
        print(f"never connected to the NT server at {host}:{port}", file=sys.stderr)
        return 1

    pacer = Pacer(rate_hz)
    for seq in range(count):
        pacer.wait()
        publisher.set(encode(size, seq))
        inst.flush()
    print(f"sent {count} messages of {size} B")
    publisher.close()
    inst.stopClient()
    return 0


def subscribe(port, payload, samples, warmup):
    size = max(payload, HEADER_LEN)
    inst = ntcore.NetworkTableInstance.create()
    inst.startServer("", "", 0, port)

    subscriber = inst.getRawTopic(TOPIC).subscribe("raw", b"", *options())
    recorder = Recorder(samples, warmup)
    print(f"NT4 server on port {port}, waiting for {samples} samples...")
    print(f"config       {config_description()}")

    deadline = time.time() + 120
    while not recorder.full() and time.time() < deadline:
        updates = subscriber.readQueue()
        if not updates:
            continue
        for update in updates:
            sample = decode(update.value)
            if sample is not None:
                recorder.record(*sample)

    recorder.report(f"ntcore v{ntcore.__version__}", size)
    subscriber.close()
    inst.stopServer()
    return 0


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)

    pub = sub.add_parser("publisher")
    pub.add_argument("--host", default="127.0.0.1")
    pub.add_argument("--port", type=int, required=True)
    pub.add_argument("--payload", type=int, default=16)
    pub.add_argument("--rate", type=int, default=500)
    pub.add_argument("--count", type=int, default=12000)

    rec = sub.add_parser("subscriber")
    rec.add_argument("--port", type=int, required=True)
    rec.add_argument("--payload", type=int, default=16)
    rec.add_argument("--samples", type=int, default=3000)
    rec.add_argument("--warmup", type=int, default=500)

    args = parser.parse_args()
    if args.command == "publisher":
        return publish(args.host, args.port, args.payload, args.rate, args.count)
    return subscribe(args.port, args.payload, args.samples, args.warmup)


if __name__ == "__main__":
    sys.exit(main())
