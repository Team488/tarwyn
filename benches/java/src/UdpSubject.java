import java.net.InetSocketAddress;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.channels.DatagramChannel;

public final class UdpSubject {
    public static final int MAX_DATAGRAM = 65507;

    private UdpSubject() {}

    private static int checkPayload(int payload) {
        int size = Math.max(payload, Harness.HEADER_LEN);
        if (size > MAX_DATAGRAM) {
            throw new IllegalArgumentException(
                "payload " + size + " B exceeds the " + MAX_DATAGRAM
                    + " B UDP datagram limit; payloads this large belong on a stream transport");
        }
        return size;
    }

    public static void publish(String host, int port, int payload, long rateHz, long count)
            throws Exception {
        int size = checkPayload(payload);
        try (DatagramChannel channel = DatagramChannel.open()) {
            channel.connect(new InetSocketAddress(host, port));
            ByteBuffer buffer = ByteBuffer.allocateDirect(size).order(ByteOrder.LITTLE_ENDIAN);
            Harness.Pacer pacer = new Harness.Pacer(rateHz);

            for (long seq = 0; seq < count; seq++) {
                pacer.await();
                buffer.clear();
                buffer.putLong(0, seq);
                buffer.putLong(8, Harness.nowNanos());
                buffer.position(0).limit(size);
                channel.write(buffer);
            }
            System.out.printf("sent %d messages of %d B%n", count, size);
        }
    }

    public static void subscribe(String host, int port, int payload, int samples) throws Exception {
        int size = checkPayload(payload);
        try (DatagramChannel channel = DatagramChannel.open()) {
            channel.bind(new InetSocketAddress(host, port));
            ByteBuffer buffer = ByteBuffer.allocateDirect(MAX_DATAGRAM).order(ByteOrder.LITTLE_ENDIAN);
            Harness.Recorder recorder = new Harness.Recorder(samples);

            System.out.printf("listening on %s:%d, waiting for %d samples...%n", host, port, samples);
            while (recorder.size() < samples) {
                buffer.clear();
                if (channel.receive(buffer) == null) {
                    continue;
                }
                if (buffer.position() < Harness.HEADER_LEN) {
                    continue;
                }
                recorder.record(buffer.getLong(0), buffer.getLong(8));
            }
            recorder.report("java-udp", size);
        }
    }
}
