import struct
import sys
import time

HEADER_LEN = 16


if hasattr(time, "clock_gettime_ns"):

    def now_nanos() -> int:
        """Wall-clock nanoseconds, from CLOCK_REALTIME where the platform has it."""
        return time.clock_gettime_ns(time.CLOCK_REALTIME)

else:

    def now_nanos() -> int:
        """Wall-clock nanoseconds from the portable clock, the same clock used elsewhere."""
        return time.time_ns()


def encode(size: int, seq: int, due_nanos: int) -> bytes:
    """Stamp a sample with its sequence number and the time it was DUE to be
    sent, never the time the send actually happened. A send delayed by a
    stalled transport must carry the delay it waited out, or the stall is
    deleted from the measurement instead of appearing in it."""
    buf = bytearray(max(size, HEADER_LEN))
    struct.pack_into("<Q", buf, 0, seq)
    struct.pack_into("<Q", buf, 8, due_nanos)
    return bytes(buf)


def decode(buf: bytes) -> tuple[int, int] | None:
    if len(buf) < HEADER_LEN:
        return None
    return struct.unpack_from("<Q", buf, 0)[0], struct.unpack_from("<Q", buf, 8)[0]


class Pacer:
    """Paces a send loop on a schedule that never slips: a send that comes back
    late leaves the following deadlines where they were, so the loop catches
    up. Skipping the missed slots would hide the stall worth seeing."""

    def __init__(self, rate_hz: int) -> None:
        self.interval_nanos = 1_000_000_000 // max(rate_hz, 1)
        self.next = time.monotonic_ns()

    def wait(self) -> int:
        """Block until the next send is due and return the wall-clock time it was due.

        Reads both clocks and subtracts the monotonic overshoot, since a
        wall-clock counter would drift from NTP by tens of microseconds."""
        self.next += self.interval_nanos
        delay = (self.next - time.monotonic_ns()) / 1e9
        if delay > 0:
            time.sleep(delay)
        overshoot = max(0, time.monotonic_ns() - self.next)
        return now_nanos() - overshoot


class Samples:
    """Collects one line per received sample for `bench row` to reduce.

    The Rust harness does all the arithmetic, so every row is computed alike.
    Lines print after the run, since printing in the loop would be measured."""

    def __init__(self, wanted: int) -> None:
        self.wanted = wanted
        self.lines: list[tuple[int, int, int]] = []

    def record(self, seq: int, due_nanos: int, received_nanos: int) -> None:
        self.lines.append((seq, due_nanos, received_nanos))

    def full(self) -> bool:
        return len(self.lines) >= self.wanted

    def emit(self) -> None:
        out = "".join(f"S\t{s}\t{d}\t{r}\n" for s, d, r in self.lines)
        sys.stdout.write(out)
        sys.stdout.flush()
