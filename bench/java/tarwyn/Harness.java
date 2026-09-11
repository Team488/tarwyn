package tarwyn;

import java.time.Instant;

public final class Harness {
    public static String version(String variable) {
        String value = System.getenv(variable);
        return value == null ? "unknown" : value;
    }

    public static final int HEADER_LEN = 16;

    private Harness() {}

    public static long nowNanos() {
        Instant now = Instant.now();
        return now.getEpochSecond() * 1_000_000_000L + now.getNano();
    }

    /** Little-endian, matching the Rust and Python harnesses' sample header. */
    public static void writeLong(byte[] buffer, int offset, long value) {
        for (int i = 0; i < 8; i++) {
            buffer[offset + i] = (byte) (value >>> (8 * i));
        }
    }

    public static long readLong(byte[] buffer, int offset) {
        long value = 0;
        for (int i = 0; i < 8; i++) {
            value |= (buffer[offset + i] & 0xFFL) << (8 * i);
        }
        return value;
    }

    public static long deadlineMillis() {
        String configured = System.getenv("BENCH_DEADLINE_SECS");
        return (configured == null ? 60L : Long.parseLong(configured)) * 1000L;
    }

    /**
     * Collects one line per received sample for {@code bench row} to reduce.
     *
     * Nothing is computed here on purpose. Percentiles, warmup, loss and the
     * achieved rate are all decided by the Rust harness, so this row and a row
     * measured through this repo's own client are the same arithmetic over
     * different transports. Lines are held until the run ends: printing inside
     * the subscribe callback would measure the print.
     */
    public static final class Samples {
        private final long[] seqs;
        private final long[] due;
        private final long[] received;
        private int count = 0;

        public Samples(int wanted) {
            this.seqs = new long[wanted];
            this.due = new long[wanted];
            this.received = new long[wanted];
        }

        public synchronized void record(long seq, long dueNanos, long receivedNanos) {
            if (count == seqs.length) {
                return;
            }
            seqs[count] = seq;
            due[count] = dueNanos;
            received[count] = receivedNanos;
            count++;
        }

        public synchronized int size() {
            return count;
        }

        public synchronized void emit() {
            StringBuilder out = new StringBuilder(count * 48);
            for (int i = 0; i < count; i++) {
                out.append("S\t").append(seqs[i]).append('\t')
                   .append(due[i]).append('\t').append(received[i]).append('\n');
            }
            System.out.print(out);
            System.out.flush();
        }
    }

    /**
     * Paces a send loop on a schedule that never slips, handing out the time
     * each send was due. Stamp that into the sample rather than the time the
     * send actually happened: the gap between the two is the delay a real
     * publisher would have suffered, and it belongs in the measurement.
     */
    public static final class Pacer {
        private final long intervalNanos;
        private long next;

        public Pacer(long rateHz) {
            this.intervalNanos = 1_000_000_000L / Math.max(1, rateHz);
            this.next = System.nanoTime();
        }

        public long await() {
            next += intervalNanos;
            while (true) {
                long remaining = next - System.nanoTime();
                if (remaining <= 0) {
                    break;
                }
                if (remaining > 1_000_000L) {
                    try {
                        Thread.sleep((remaining - 1_000_000L) / 1_000_000L);
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        break;
                    }
                } else {
                    Thread.onSpinWait();
                }
            }
            return dueWallClock();
        }

        /**
         * The wall-clock time the deadline just waited for fell at.
         *
         * Both clocks are read together and the monotonic overshoot taken off,
         * rather than advancing a wall-clock counter alongside the schedule.
         * NTP disciplines the wall clock and leaves the monotonic one alone, so
         * a counter advanced in step with the schedule drifts tens of
         * microseconds away from the clock the subscriber stamps with, and once
         * that drift exceeds the latency the samples read as negative.
         */
        private long dueWallClock() {
            long overshoot = Math.max(0L, System.nanoTime() - next);
            return nowNanos() - overshoot;
        }
    }
}
