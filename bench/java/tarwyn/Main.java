package tarwyn;

import java.util.HashMap;
import java.util.Map;

public final class Main {
    private static String usage() {
        return """
            usage: bench <publisher|subscriber> [options]

              --probe     client (TarwynClient) | socket (its ZeroMQ sockets)
              --host      default 127.0.0.1
              --payload   wire bytes, minimum 16 (default 16)
              --rate      publisher send rate in Hz (default 1000)
              --count     publisher message count (default 100000)
              --samples   subscriber sample target (default 100000)
            """;
    }

    private static Map<String, String> parse(String[] args) {
        Map<String, String> options = new HashMap<>();
        for (int i = 1; i + 1 < args.length; i += 2) {
            if (!args[i].startsWith("--")) {
                throw new IllegalArgumentException("expected an option, got: " + args[i]);
            }
            options.put(args[i].substring(2), args[i + 1]);
        }
        return options;
    }

    public static void main(String[] args) throws Exception {
        if (args.length == 0) {
            System.out.print(usage());
            System.exit(2);
        }
        Map<String, String> options = parse(args);
        String host = options.getOrDefault("host", "127.0.0.1");
        int payload = Integer.parseInt(options.getOrDefault("payload", "16"));
        String probeName = options.getOrDefault("probe", "client");
        Probe probe = switch (probeName) {
            case "client" -> new TarwynClientProbe();
            case "socket" -> new TarwynSocketProbe();
            default -> null;
        };
        if (probe == null) {
            System.err.println("unknown probe: " + probeName);
            System.exit(2);
            return;
        }
        long rate = Long.parseLong(options.getOrDefault("rate", "1000"));
        long count = Long.parseLong(options.getOrDefault("count", "100000"));
        int samples = Integer.parseInt(options.getOrDefault("samples", "100000"));

        switch (args[0]) {
            case "publisher" -> probe.publish(host, payload, rate, count);
            case "subscriber" -> probe.subscribe(host, payload, samples);
            default -> {
                System.err.println("unknown command: " + args[0]);
                System.out.print(usage());
                System.exit(2);
            }
        }
    }
}
