import struct
import time

HEADER_LEN = 16


def now_nanos():
    return time.clock_gettime_ns(time.CLOCK_REALTIME)


def encode(size, seq):
    buf = bytearray(max(size, HEADER_LEN))
    struct.pack_into("<Q", buf, 0, seq)
    struct.pack_into("<Q", buf, 8, now_nanos())
    return bytes(buf)


def decode(buf):
    if len(buf) < HEADER_LEN:
        return None
    return struct.unpack_from("<Q", buf, 0)[0], struct.unpack_from("<Q", buf, 8)[0]


class Pacer:
    def __init__(self, rate_hz):
        self.interval = 1.0 / max(rate_hz, 1)
        self.next = time.perf_counter()

    def wait(self):
        self.next += self.interval
        delay = self.next - time.perf_counter()
        if delay > 0:
            time.sleep(delay)


class Recorder:
    def __init__(self, samples, warmup):
        self.samples = samples
        self.warmup = warmup
        self.discarded = 0
        self.latencies = []
        self.gaps = 0
        self.reordered = 0
        self.highest_seq = None
        self.first_seq = None

    def record(self, seq, sent_nanos):
        if self.highest_seq is not None:
            if seq > self.highest_seq + 1:
                self.gaps += seq - self.highest_seq - 1
            elif seq <= self.highest_seq:
                self.reordered += 1
        if self.highest_seq is None or seq > self.highest_seq:
            self.highest_seq = seq
        if self.discarded < self.warmup:
            self.discarded += 1
            return
        if self.first_seq is None:
            self.first_seq = seq
        self.latencies.append(now_nanos() - sent_nanos)

    def full(self):
        return len(self.latencies) >= self.samples

    def report(self, subject, payload):
        if not self.latencies:
            print(f"{subject} @ {payload}B: no samples received")
            return
        ordered = sorted(self.latencies)

        def at(q):
            index = min(int(len(ordered) * q), len(ordered) - 1)
            return ordered[index] / 1000.0

        sent = len(ordered) + self.gaps
        loss = 100.0 * self.gaps / sent if sent else 0.0
        row = [
            f"{at(0.50):.2f}",
            f"{ordered[0] / 1000.0:.2f}",
            f"{at(0.80):.2f}",
            f"{at(0.90):.2f}",
            f"{at(0.95):.2f}",
            f"{at(0.99):.2f}",
            f"{at(0.999):.2f}",
            f"{ordered[-1] / 1000.0:.2f}",
            f"{loss:.2f}",
        ]
        print("ROW\t" + "\t".join([subject, str(payload)] + row))
        print(f"subject      {subject}")
        print(f"payload      {payload} B")
        print(f"received     {len(ordered)}")
        print(f"dropped      {self.gaps} (gaps in sequence)")
        print(f"reordered    {self.reordered}")
        print(f"median       {at(0.50):>9.2f} us")
        print(f"p99          {at(0.99):>9.2f} us")
        print(f"p99.9        {at(0.999):>9.2f} us")
        print(f"loss         {loss:>9.2f} %")
