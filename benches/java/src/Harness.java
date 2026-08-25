import java.time.Instant;
import java.util.Arrays;

public final class Harness {
    public static final int HEADER_LEN = 16;

    private Harness() {}

    public static long nowNanos() {
        Instant now = Instant.now();
        return now.getEpochSecond() * 1_000_000_000L + now.getNano();
    }

    public static final class Recorder {
        private final long[] latencies;
        private int count = 0;
        private long highestSeq = -1;
        private long gaps = 0;
        private long reordered = 0;

        public Recorder(int capacity) {
            this.latencies = new long[capacity];
        }

        public void record(long seq, long sentNanos) {
            if (count == latencies.length) {
                return;
            }
            latencies[count++] = Math.max(0, nowNanos() - sentNanos);
            if (highestSeq >= 0) {
                if (seq > highestSeq + 1) {
                    gaps += seq - highestSeq - 1;
                } else if (seq <= highestSeq) {
                    reordered++;
                }
            }
            if (seq > highestSeq) {
                highestSeq = seq;
            }
        }

        public int size() {
            return count;
        }

        private double quantileUs(long[] sorted, double q) {
            if (sorted.length == 0) {
                return 0;
            }
            int index = (int) Math.ceil(q * sorted.length) - 1;
            index = Math.max(0, Math.min(sorted.length - 1, index));
            return sorted[index] / 1000.0;
        }

        public void report(String subject, int payload) {
            if (count == 0) {
                System.out.printf("%s @ %dB: no samples received%n", subject, payload);
                return;
            }
            long[] sorted = Arrays.copyOf(latencies, count);
            Arrays.sort(sorted);
            System.out.printf("subject      %s%n", subject);
            System.out.printf("payload      %d B%n", payload);
            System.out.printf("received     %d%n", count);
            System.out.printf("dropped      %d (gaps in sequence)%n", gaps);
            System.out.printf("reordered    %d%n", reordered);
            System.out.printf("p50          %9.2f us%n", quantileUs(sorted, 0.50));
            System.out.printf("p99          %9.2f us%n", quantileUs(sorted, 0.99));
            System.out.printf("p999         %9.2f us%n", quantileUs(sorted, 0.999));
            System.out.printf("max          %9.2f us%n", sorted[sorted.length - 1] / 1000.0);
        }
    }

    public static final class Pacer {
        private final long intervalNanos;
        private long next;

        public Pacer(long rateHz) {
            this.intervalNanos = 1_000_000_000L / Math.max(1, rateHz);
            this.next = System.nanoTime();
        }

        public void await() {
            next += intervalNanos;
            while (true) {
                long remaining = next - System.nanoTime();
                if (remaining <= 0) {
                    return;
                }
                if (remaining > 1_000_000L) {
                    try {
                        Thread.sleep((remaining - 1_000_000L) / 1_000_000L);
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        return;
                    }
                } else {
                    Thread.onSpinWait();
                }
            }
        }
    }
}
