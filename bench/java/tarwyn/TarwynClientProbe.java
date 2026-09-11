package tarwyn;

import org.team488.JClient.TarwynClient;
import org.team488.Utilities.Entities.TarwynProto.TarwynMessage.TarwynUpdate;

/**
 * The TARWYN server driven through TarwynClient, the way a robot's own code
 * reaches it. Publishing here does not touch the wire: TarwynClient.publish
 * serializes the message into a CircularBuffer that a ConcurrentPushHandler
 * daemon drains and sends, so the difference between this probe and
 * {@link TarwynSocketProbe} is that queue.
 */
public final class TarwynClientProbe implements Probe {
    public static final String CHANNEL = "bench";

    /**
     * Publishes {@code count} messages at {@code rateHz}, stamping each with the
     * time it was due. The pacer is built only after the client has had time to
     * connect; built first, its early slots would spend the connect catching up
     * and carry that wait as latency.
     */
    @Override
    public void publish(String host, int payload, long rateHz, long count) throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        TarwynClient client = new TarwynClient(host);
        byte[] buffer = new byte[size];

        Thread.sleep(1500);
        Harness.Pacer pacer = new Harness.Pacer(rateHz);

        for (long seq = 0; seq < count; seq++) {
            long due = pacer.await();
            Harness.writeLong(buffer, 0, seq);
            Harness.writeLong(buffer, 8, due);
            client.publish(CHANNEL, buffer);
        }
        System.out.printf("sent %d messages of %d B%n", count, size);
        client.shutdown();
    }

    @Override
    public void subscribe(String host, int payload, int samples) throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        Harness.Samples collected = new Harness.Samples(samples);

        TarwynClient client = new TarwynClient(host);
        client.subscribe(CHANNEL, (TarwynUpdate update) -> {
            long received = Harness.nowNanos();
            byte[] value = update.getValue().toByteArray();
            if (value.length >= Harness.HEADER_LEN) {
                collected.record(Harness.readLong(value, 0), Harness.readLong(value, 8), received);
            }
        });

        System.out.printf("subscribed to '%s' on %s, waiting for %d samples...%n",
            CHANNEL, host, samples);
        long deadline = System.currentTimeMillis() + Harness.deadlineMillis();
        while (collected.size() < samples && System.currentTimeMillis() < deadline) {
            Thread.onSpinWait();
        }
        collected.emit();
        System.err.printf("version      %s, payload %d B%n",
            Harness.version("BENCH_TARWYN_VERSION"), size);
        client.shutdown();
    }


}
