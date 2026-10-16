"""Record every NT topic a server announces, with type, properties, update rate and size range.

Usage: capture.py <seconds> [host] > capture.json

The capture feeds `bench profile from-capture`. Counting starts half a second after
the connection, past the first burst of announcements and values. Sizes count a value
as msgpack, and the byte rate adds the 14 bytes of NT4 frame around each one: the
array header, topic id, timestamp and type.
"""

import json
import sys
import time
from collections import defaultdict

import ntcore

from ntcompat import EVENT_TOPIC, EVENT_VALUE_ALL, call, options

FRAME_OVERHEAD = 1 + 3 + 9 + 1


def value_bytes(v):
    """Rough msgpack size of a value; exact enough to rank topics and size payloads."""
    if isinstance(v, bool):
        return 1
    if isinstance(v, (int, float)):
        return 9
    if isinstance(v, str):
        return len(v.encode()) + 3
    if isinstance(v, (bytes, bytearray)):
        return len(v) + 3
    if isinstance(v, (list, tuple)):
        return 3 + sum(value_bytes(x) for x in v)
    return 9


def main():
    seconds = float(sys.argv[1]) if len(sys.argv) > 1 else 60
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"

    inst = ntcore.NetworkTableInstance.create()
    call(inst, "start_client", "nt-capture")
    call(inst, "set_server", host)
    poller = ntcore.NetworkTableListenerPoller(inst)
    call(poller, "add_listener", [""], EVENT_TOPIC | EVENT_VALUE_ALL)
    sub = ntcore.MultiSubscriber(inst, [""], options(send_all=True))

    deadline = time.monotonic() + 30
    while not call(inst, "is_connected"):
        if time.monotonic() > deadline:
            sys.exit(f"no NT server at {host}")
        time.sleep(0.05)
    time.sleep(0.5)
    call(poller, "read_queue")

    updates = defaultdict(int)
    total = defaultdict(int)
    smallest = {}
    largest = {}
    start = time.monotonic()
    while time.monotonic() - start < seconds:
        for ev in call(poller, "read_queue"):
            if not ev.is_(EVENT_VALUE_ALL):
                continue
            name = call(ev.data.topic, "get_name")
            size = value_bytes(ev.data.value.value())
            updates[name] += 1
            total[name] += FRAME_OVERHEAD + size
            smallest[name] = min(smallest.get(name, size), size)
            largest[name] = max(largest.get(name, size), size)
        time.sleep(0.002)
    elapsed = time.monotonic() - start

    rows = []
    for topic in call(inst, "get_topics"):
        name = call(topic, "get_name")
        info = call(topic, "get_info")
        rows.append(
            {
                "topic": name,
                "type": call(topic, "get_type_string"),
                "properties": json.loads(info.properties or "{}"),
                "hz": round(updates[name] / elapsed, 2),
                "bytes": [smallest.get(name, 0), largest.get(name, 0)],
                "bytes_per_s": round(total[name] / elapsed),
            }
        )
    rows.sort(key=lambda r: (-r["bytes_per_s"], r["topic"]))
    json.dump(
        {
            "seconds": round(elapsed, 1),
            "topics_announced": len(rows),
            "topics_updating": sum(1 for r in rows if r["hz"] > 0),
            "updates_per_s": round(sum(updates.values()) / elapsed),
            "bytes_per_s": round(sum(total.values()) / elapsed),
            "topics": rows,
        },
        sys.stdout,
        indent=1,
    )
    sub.close()
    call(inst, "stop_client")


if __name__ == "__main__":
    main()
