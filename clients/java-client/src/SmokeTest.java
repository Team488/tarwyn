import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.List;

public final class SmokeTest {
    private static int failures = 0;

    private static void check(boolean condition, String what) {
        if (condition) {
            System.out.println("  ok    " + what);
        } else {
            System.out.println("  FAIL  " + what);
            failures++;
        }
    }

    public static void main(String[] args) throws Exception {
        Path library = Path.of(args.length > 0 ? args[0] : "../../target/release/libtarwyn_ffi.so");
        System.out.println("library " + library.toAbsolutePath());

        try (TarwynClient client = new TarwynClient(library, "127.0.0.1")) {
            check(true, "client constructed without a server running");

            client.publish("smoke/double", 1.5);
            client.publish("smoke/bool", true);
            client.publish("smoke/string", "hello");
            client.publish("smoke/bytes", "raw".getBytes(StandardCharsets.UTF_8));
            check(true, "publishes do not block or throw");

            check(client.getDouble("smoke/absent") == null,
                "get on an absent channel returns null rather than blocking");

            client.start();

            try (TarwynClient.Subscription subscription =
                     client.subscribe("smoke/ring", 64, 256)) {
                check(subscription.writeIndex() == 0, "a fresh ring has write index 0");
                check(subscription.drain().isEmpty(), "a fresh ring drains empty");
                check(!subscription.lapped(), "a fresh ring has not lapped");

                byte[] payload = "round-trip".getBytes(StandardCharsets.UTF_8);
                List<byte[]> seen = List.of();
                for (int attempt = 0; attempt < 40 && seen.isEmpty(); attempt++) {
                    client.publish("smoke/ring", payload);
                    Thread.sleep(100);
                    seen = subscription.drain();
                }
                check(!seen.isEmpty(), "a published value reaches the ring");
                if (!seen.isEmpty()) {
                    check(TarwynClient.utf8(seen.get(0)).equals("round-trip"),
                        "the payload survives the round trip intact");
                }
            }

            System.out.println("  info  dropped publishes: " + client.droppedPublishes());
        }

        System.out.println(failures == 0 ? "SMOKE PASS" : "SMOKE FAIL (" + failures + ")");
        System.exit(failures == 0 ? 0 : 1);
    }
}
