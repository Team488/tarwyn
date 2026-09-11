package tarwyn;

import com.google.protobuf.ByteString;
import org.team488.Utilities.Entities.TarwynProto.TarwynMessage;
import org.team488.Utilities.Entities.TarwynProto.TarwynMessage.TarwynUpdate;
import org.zeromq.SocketType;
import org.zeromq.ZContext;
import org.zeromq.ZMQ;

/**
 * The TARWYN server driven straight over its own ZeroMQ sockets, with
 * TarwynClient taken out of the path.
 *
 * TarwynClient.publish does not touch the wire: it serializes the message and
 * writes it into a CircularBuffer that a ConcurrentPushHandler daemon drains
 * and sends. This probe sends the identical bytes from the publishing thread
 * instead, so the difference between this row and the TarwynClient row is that
 * queue and the handoff around it, with the server, the transport, the JVM and
 * the message on the wire all held constant.
 *
 * The frames are the ones their own client sends and receives, read out of
 * its bytecode: one PUSH frame carrying a serialized TarwynMessage, one SUB
 * frame carrying a serialized TarwynUpdate.
 */
public final class TarwynSocketProbe implements Probe {
    public static final String CHANNEL = "bench";

    private static final int PUSH_PORT = 48800;
    private static final int SUBSCRIBE_PORT = 48802;

    private static byte[] encode(String key, byte[] value) {
        return TarwynMessage.newBuilder()
            .setKey(key)
            .setCommand(TarwynMessage.Command.PUBLISH)
            .setValue(ByteString.copyFrom(value))
            .build()
            .toByteArray();
    }

    /**
     * Publishes {@code count} messages at {@code rateHz}, stamping each with the
     * time it was due. The pacer is built only after the socket has had time to
     * connect; built first, its early slots would spend the connect catching up
     * and carry that wait as latency.
     */
    @Override
    public void publish(String host, int payload, long rateHz, long count) throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        try (ZContext context = new ZContext()) {
            ZMQ.Socket push = context.createSocket(SocketType.PUSH);
            push.connect("tcp://" + host + ":" + PUSH_PORT);

            byte[] buffer = new byte[size];
            Thread.sleep(1500);
            Harness.Pacer pacer = new Harness.Pacer(rateHz);

            for (long seq = 0; seq < count; seq++) {
                long due = pacer.await();
                Harness.writeLong(buffer, 0, seq);
                Harness.writeLong(buffer, 8, due);
                push.send(encode(CHANNEL, buffer), 0);
            }
            System.out.printf("sent %d messages of %d B%n", count, size);
        }
    }

    /**
     * Collects {@code samples} updates and prints them for {@code bench row}.
     * The subscription is to everything rather than to a prefix: one topic is
     * in flight, and a prefix filter over a protobuf frame would be a guess
     * about the encoding rather than a measurement.
     */
    @Override
    public void subscribe(String host, int payload, int samples) throws Exception {
        int size = Math.max(payload, Harness.HEADER_LEN);
        Harness.Samples collected = new Harness.Samples(samples);

        try (ZContext context = new ZContext()) {
            ZMQ.Socket sub = context.createSocket(SocketType.SUB);
            sub.connect("tcp://" + host + ":" + SUBSCRIBE_PORT);
            sub.subscribe(new byte[0]);
            sub.setReceiveTimeOut(200);

            System.out.printf("subscribed to '%s' on %s:%d, waiting for %d samples...%n",
                CHANNEL, host, SUBSCRIBE_PORT, samples);
            System.out.flush();

            long deadline = System.currentTimeMillis() + Harness.deadlineMillis();
            while (collected.size() < samples && System.currentTimeMillis() < deadline) {
                byte[] frame = sub.recv();
                if (frame == null) {
                    continue;
                }
                long received = Harness.nowNanos();
                TarwynUpdate update = TarwynUpdate.parseFrom(frame);
                byte[] value = update.getValue().toByteArray();
                if (value.length >= Harness.HEADER_LEN) {
                    collected.record(Harness.readLong(value, 0), Harness.readLong(value, 8), received);
                }
            }
            collected.emit();
            System.err.printf("version      %s, payload %d B%n",
                Harness.version("BENCH_TARWYN_VERSION"), size);
        }
    }


}
