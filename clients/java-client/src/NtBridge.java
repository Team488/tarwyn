import edu.wpi.first.networktables.NetworkTableInstance;
import edu.wpi.first.networktables.RawPublisher;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class NtBridge implements AutoCloseable {
    private final TarwynClient client;
    private final NetworkTableInstance instance;
    private final Map<String, RawPublisher> publishers = new LinkedHashMap<>();
    private final Map<String, TarwynClient.Subscription> subscriptions = new LinkedHashMap<>();
    private final String prefix;
    private boolean closed = false;

    public NtBridge(TarwynClient client, NetworkTableInstance instance, String prefix) {
        this.client = client;
        this.instance = instance;
        this.prefix = prefix;
    }

    public void mirror(String channel, int records, int recordBytes) {
        if (subscriptions.containsKey(channel)) {
            return;
        }
        subscriptions.put(channel, client.subscribe(channel, records, recordBytes));
        publishers.put(channel, instance.getRawTopic(prefix + channel).publish("raw"));
    }

    public int pump() {
        if (closed) {
            return 0;
        }
        if (!hasDashboard()) {
            for (TarwynClient.Subscription subscription : subscriptions.values()) {
                subscription.drain();
            }
            return 0;
        }
        int mirrored = 0;
        for (Map.Entry<String, TarwynClient.Subscription> entry : subscriptions.entrySet()) {
            RawPublisher publisher = publishers.get(entry.getKey());
            for (byte[] payload : entry.getValue().drain()) {
                publisher.set(payload);
                mirrored++;
            }
        }
        if (mirrored > 0) {
            instance.flush();
        }
        return mirrored;
    }

    public boolean hasDashboard() {
        return instance.getConnections().length > 0;
    }

    public List<String> laggingChannels() {
        List<String> lagging = new ArrayList<>();
        for (Map.Entry<String, TarwynClient.Subscription> entry : subscriptions.entrySet()) {
            if (entry.getValue().lapped()) {
                lagging.add(entry.getKey());
            }
        }
        return lagging;
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }
        closed = true;
        for (RawPublisher publisher : publishers.values()) {
            publisher.close();
        }
        for (TarwynClient.Subscription subscription : subscriptions.values()) {
            subscription.close();
        }
        publishers.clear();
        subscriptions.clear();
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("usage: NtBridge <libtarwyn_ffi.so> <host> [channel...]");
            System.exit(2);
        }
        Path library = Path.of(args[0]);
        String host = args[1];

        NetworkTableInstance instance = NetworkTableInstance.getDefault();
        instance.startServer();

        try (TarwynClient client = new TarwynClient(library, host)) {
            try (NtBridge bridge = new NtBridge(client, instance, "/tarwyn/")) {
                for (int i = 2; i < args.length; i++) {
                    bridge.mirror(args[i], 256, 4096);
                }
                client.start();
                System.out.println("bridging " + (args.length - 2) + " channels from " + host);

                while (!Thread.currentThread().isInterrupted()) {
                    bridge.pump();
                    List<String> lagging = bridge.laggingChannels();
                    if (!lagging.isEmpty()) {
                        System.out.println("ring lapped, values dropped: " + lagging);
                    }
                    Thread.sleep(20);
                }
            }
        } finally {
            instance.stopServer();
        }
    }
}
