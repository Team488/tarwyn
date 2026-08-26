package tarwyn;

import org.team488.JClient.TarwynClient;
import org.team488.Utilities.Entities.TarwynProto.TarwynMessage.TarwynUpdate;

public final class TarwynSubject {
    public static final String CHANNEL = "bench";

    private TarwynSubject() {}

    public static void publish(String host, int payload, long rateHz, long count) throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        TarwynClient client = new TarwynClient(host);
        byte[] buffer = new byte[size];
        Harness.Pacer pacer = new Harness.Pacer(rateHz);

        Thread.sleep(1500);

        for (long seq = 0; seq < count; seq++) {
            pacer.await();
            writeLong(buffer, 0, seq);
            writeLong(buffer, 8, Harness.nowNanos());
            client.publish(CHANNEL, buffer);
        }
        System.out.printf("sent %d messages of %d B%n", count, size);
        client.shutdown();
    }

    public static void subscribe(String host, int payload, int samples) throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        Harness.Recorder recorder = new Harness.Recorder(samples);

        TarwynClient client = new TarwynClient(host);
        client.subscribe(CHANNEL, (TarwynUpdate update) -> {
            byte[] value = update.getValue().toByteArray();
            if (value.length >= Harness.HEADER_LEN) {
                recorder.record(readLong(value, 0), readLong(value, 8));
            }
        });

        System.out.printf("subscribed to '%s' on %s, waiting for %d samples...%n",
            CHANNEL, host, samples);
        long deadline = System.currentTimeMillis() + 120_000;
        while (recorder.size() < samples && System.currentTimeMillis() < deadline) {
            Thread.onSpinWait();
        }
        recorder.report("tarwyn", size);
        client.shutdown();
    }

    private static void writeLong(byte[] buffer, int offset, long value) {
        for (int i = 0; i < 8; i++) {
            buffer[offset + i] = (byte) (value >>> (8 * i));
        }
    }

    private static long readLong(byte[] buffer, int offset) {
        long value = 0;
        for (int i = 0; i < 8; i++) {
            value |= (buffer[offset + i] & 0xFFL) << (8 * i);
        }
        return value;
    }
}
