import sys
import time

import tarwyn

failures = 0


def check(condition, what):
    global failures
    print(("  ok    " if condition else "  FAIL  ") + what)
    if not condition:
        failures += 1


client = tarwyn.TarwynClient(host="127.0.0.1")
check(True, "client constructed")

client.putDouble("smoke/double", 1.5)
client.putStringList("smoke/list", ["a", "b"])
check(True, "camelCase publishes do not block")

check(client.getDouble("smoke/absent") is None, "getDouble on an absent channel returns None")

seen = []
client.start()
check(client.subscribe("smoke/round", seen.append) is None, "subscribe(channel, callback)")

for _ in range(40):
    client.putBytes("smoke/round", b"ping")
    time.sleep(0.1)
    if seen:
        break

check(bool(seen), "callback receives published values")
if seen:
    check(seen[0] == b"ping", "payload intact through the callback path")

check(client.unsubscribe("smoke/round", seen.append) is False, "unsubscribe of an unknown callback is False")

with client.subscribe_buffered("smoke/buffered", depth=8) as subscription:
    check(len(subscription) == 0, "buffered subscription starts empty")

print("  info  dropped publishes:", client.droppedPublishes())
client.stop()

print("PY SMOKE PASS" if failures == 0 else f"PY SMOKE FAIL ({failures})")
sys.exit(0 if failures == 0 else 1)
