import edu.wpi.first.networktables.NetworkTableInstance;
import edu.wpi.first.networktables.PubSubOption;
import edu.wpi.first.networktables.RawPublisher;
import edu.wpi.first.networktables.RawSubscriber;
import edu.wpi.first.networktables.RawTopic;
import edu.wpi.first.networktables.TimestampedRaw;

public final class Nt4Subject {
    public static final String TOPIC = "/bench/payload";
    private static final double PERIODIC_SECONDS = 0.001;

    private Nt4Subject() {}

    public static String configDescription() {
        return "sendAll(true), keepDuplicates(true), periodic(" + PERIODIC_SECONDS
            + "s), pollStorage(1000), flush() after every set, read via readQueue()";
    }

    private static PubSubOption[] options(int pollStorage) {
        return new PubSubOption[] {
            PubSubOption.sendAll(true),
            PubSubOption.keepDuplicates(true),
            PubSubOption.periodic(PERIODIC_SECONDS),
            PubSubOption.pollStorage(pollStorage),
        };
    }

    public static void publish(String host, int port, int payload, long rateHz, long count)
            throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        NetworkTableInstance inst = NetworkTableInstance.create();
        inst.startClient4("bench-publisher");
        inst.setServer(host, port);

        RawTopic topic = inst.getRawTopic(TOPIC);
        RawPublisher publisher = topic.publish("raw", options(1000));

        byte[] buffer = new byte[size];
        Harness.Pacer pacer = new Harness.Pacer(rateHz);

        long connectDeadline = System.currentTimeMillis() + 10_000;
        while (!inst.isConnected() && System.currentTimeMillis() < connectDeadline) {
            Thread.sleep(20);
        }
        if (!inst.isConnected()) {
            System.err.println("never connected to the NT server at " + host + ":" + port);
            System.exit(1);
        }

        for (long seq = 0; seq < count; seq++) {
            pacer.await();
            writeLong(buffer, 0, seq);
            writeLong(buffer, 8, Harness.nowNanos());
            publisher.set(buffer);
            inst.flush();
        }
        System.out.printf("sent %d messages of %d B%n", count, size);
        publisher.close();
        inst.stopClient();
        inst.close();
    }

    public static void subscribe(int port, int payload, int samples) throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        NetworkTableInstance inst = NetworkTableInstance.create();
        inst.startServer("", "", 0, port);

        RawTopic topic = inst.getRawTopic(TOPIC);
        RawSubscriber subscriber = topic.subscribe("raw", new byte[0], options(1000));
        Harness.Recorder recorder = new Harness.Recorder(samples);

        System.out.printf("NT4 server on port %d, waiting for %d samples...%n", port, samples);
        System.out.printf("config       %s%n", configDescription());

        long deadline = System.currentTimeMillis() + 120_000;
        while (recorder.size() < samples && System.currentTimeMillis() < deadline) {
            TimestampedRaw[] updates = subscriber.readQueue();
            if (updates.length == 0) {
                Thread.onSpinWait();
                continue;
            }
            for (TimestampedRaw update : updates) {
                if (update.value.length >= Harness.HEADER_LEN) {
                    recorder.record(readLong(update.value, 0), readLong(update.value, 8));
                }
            }
        }
        recorder.report("nt4-flush", size);
        subscriber.close();
        inst.stopServer();
        inst.close();
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
