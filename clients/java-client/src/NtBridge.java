import edu.wpi.first.networktables.NetworkTableInstance;
import edu.wpi.first.networktables.RawPublisher;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Mirrors TARWYN channels into NetworkTables, one way.
 *
 * Exists so a driver station dashboard can watch values without TARWYN traffic
 * paying NT4's cost on the robot. Nothing flows back: NetworkTables is the
 * display, not a second source of truth.
 *
 * When no dashboard is connected the rings are still drained and the payloads
 * discarded, so a bridge nobody is watching does not lap its subscriptions and
 * report false lag.
 */
public final class NtBridge implements AutoCloseable {
    private final TarwynClient client;
    private final NetworkTableInstance instance;
    private final Map<String, RawPublisher> publishers = new LinkedHashMap<>();
    private final Map<String, TarwynClient.Subscription> subscriptions = new LinkedHashMap<>();
    private final String prefix;
    private boolean closed = false;

    /**
     * Build a bridge over an existing client and NetworkTables instance.
     *
     * @param client the client to read from
     * @param instance the NetworkTables instance to publish into
     * @param prefix prepended to every mirrored channel name
     */
    public NtBridge(TarwynClient client, NetworkTableInstance instance, String prefix) {
        this.client = client;
        this.instance = instance;
        this.prefix = prefix;
    }

    /**
     * Begin mirroring a channel. Does nothing if it is already mirrored.
     *
     * @param channel the channel to mirror
     * @param records how many payloads its ring holds
     * @param recordBytes the size of each slot, including an 8-byte length prefix
     */
    public void mirror(String channel, int records, int recordBytes) {
        if (subscriptions.containsKey(channel)) {
            return;
        }
        subscriptions.put(channel, client.subscribe(channel, records, recordBytes));
        publishers.put(channel, instance.getRawTopic(prefix + channel).publish("raw"));
    }

    /**
     * Drain every mirrored channel and publish what it found.
     *
     * Call this on a loop. Flushes NetworkTables only when something was published.
     *
     * @return how many payloads were mirrored, or 0 when closed or no dashboard is connected
     */
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

    /**
     * Whether any NetworkTables client is connected.
     *
     * @return true when at least one is
     */
    public boolean hasDashboard() {
        return instance.getConnections().length > 0;
    }

    /**
     * The mirrored channels whose rings have lapped, meaning values were published
     * faster than {@link #pump()} drained them.
     *
     * @return the lagging channel names, empty when none are
     */
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
    /**
     * Close every publisher and subscription. Safe to call twice.
     */
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

    /**
     * Run the bridge from the command line, mirroring the named channels until
     * interrupted.
     *
     * @param args the native library, the host, then the channels to mirror
     * @throws Exception if the client or NetworkTables instance cannot be started
     */
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
