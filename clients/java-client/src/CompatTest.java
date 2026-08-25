import java.nio.charset.StandardCharsets;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.List;

public final class CompatTest {
    private static int failures = 0;

    private static void check(boolean condition, String what) {
        System.out.println((condition ? "  ok    " : "  FAIL  ") + what);
        if (!condition) {
            failures++;
        }
    }

    public static void main(String[] args) throws Exception {
        TarwynClientManager manager = TarwynClientManager.getClientAsynchronously("127.0.0.1");
        check(true, "getClientAsynchronously returns without blocking");

        TarwynClient client = manager.getClientFuture().get();
        check(client != null, "getClientFuture resolves");
        check(manager.getOrNull() != null, "getOrNull returns once resolved");

        List<byte[]> seen = new CopyOnWriteArrayList<>();
        client.start();
        check(client.subscribe("compat/java", seen::add), "subscribe(channel, consumer)");

        for (int i = 0; i < 40 && seen.isEmpty(); i++) {
            client.publish("compat/java", "ping".getBytes(StandardCharsets.UTF_8));
            Thread.sleep(100);
        }
        check(!seen.isEmpty(), "consumer receives published values");
        if (!seen.isEmpty()) {
            check(new String(seen.get(0), StandardCharsets.UTF_8).equals("ping"),
                "payload intact through the consumer path");
        }
        check(client.getDouble("compat/absent") == null, "getDouble returns null when absent");
        client.shutdown();
        check(true, "shutdown()");

        System.out.println(failures == 0 ? "JAVA COMPAT PASS" : "JAVA COMPAT FAIL");
        System.exit(failures == 0 ? 0 : 1);
    }
}
