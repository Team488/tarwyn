import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

public final class RingSoak {
    public static void main(String[] args) throws Exception {
        Path library = Path.of(args[0]);
        int records = args.length > 1 ? Integer.parseInt(args[1]) : 8;
        long millis = args.length > 2 ? Long.parseLong(args[2]) : 5000;

        try (TarwynClient client = new TarwynClient(library, "127.0.0.1")) {
            client.start();
            TarwynClient.Subscription subscription = client.subscribe("ringsoak", records, 64);
            Thread.sleep(300);

            long deadline = System.currentTimeMillis() + millis;
            Thread publisher = new Thread(() -> {
                long value = 0;
                ByteBuffer buffer = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN);
                while (System.currentTimeMillis() < deadline) {
                    for (int burst = 0; burst < 50; burst++) {
                        buffer.putLong(0, value++);
                        client.publish("ringsoak", buffer.array());
                    }
                    Thread.onSpinWait();
                }
            });
            publisher.setDaemon(true);
            publisher.start();

            List<Long> seen = new ArrayList<>();
            int malformed = 0;
            while (System.currentTimeMillis() < deadline) {
                for (byte[] value : subscription.drain()) {
                    if (value.length != 8) {
                        malformed++;
                        continue;
                    }
                    seen.add(ByteBuffer.wrap(value).order(ByteOrder.LITTLE_ENDIAN).getLong());
                }
                Thread.sleep(20);
            }
            publisher.join();

            long duplicates = 0;
            long outOfOrder = 0;
            Set<Long> unique = new HashSet<>();
            for (int i = 0; i < seen.size(); i++) {
                if (!unique.add(seen.get(i))) {
                    duplicates++;
                }
                if (i > 0 && seen.get(i) <= seen.get(i - 1)) {
                    outOfOrder++;
                }
            }

            boolean lapped = subscription.lapped();
            subscription.close();

            System.out.println("received=" + seen.size()
                + " malformed=" + malformed
                + " duplicates=" + duplicates
                + " outOfOrder=" + outOfOrder
                + " lapped=" + lapped);

            if (malformed > 0 || duplicates > 0 || outOfOrder > 0) {
                throw new AssertionError("the ring handed back a value the writer had overwritten");
            }
            if (seen.isEmpty()) {
                throw new AssertionError("no values arrived, so nothing was actually tested");
            }
            System.out.println("RING PASS");
        }
    }
}
