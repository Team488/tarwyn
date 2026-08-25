import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.CompletableFuture;

public final class TarwynClientManager {
    private final CompletableFuture<TarwynClient> future;
    private volatile TarwynClient client;

    private TarwynClientManager(CompletableFuture<TarwynClient> future) {
        this.future = future;
        future.thenAccept(created -> this.client = created);
    }

    public static TarwynClientManager getDefaultClientAsynchronously() {
        return getClientAsynchronously("127.0.0.1");
    }

    public static TarwynClientManager getClientAsynchronously(String host) {
        return getClientAsynchronously(host, defaultLibrary());
    }

    public static TarwynClientManager getClientAsynchronously(String host, Path library) {
        return new TarwynClientManager(
            CompletableFuture.supplyAsync(() -> new TarwynClient(library, host)));
    }

    public CompletableFuture<TarwynClient> getClientFuture() {
        return future;
    }

    public TarwynClient getOrNull() {
        return client;
    }

    public boolean isReady() {
        return client != null;
    }

    public void shutdown() {
        TarwynClient existing = client;
        if (existing != null) {
            existing.close();
        }
    }

    static Path defaultLibrary() {
        String override = System.getProperty("tarwyn.library");
        if (override != null) {
            return Path.of(override);
        }
        List<Path> candidates = List.of(
            Path.of("target/release/libtarwyn_ffi.so"),
            Path.of("../../target/release/libtarwyn_ffi.so"),
            Path.of("/usr/local/lib/libtarwyn_ffi.so"));
        for (Path candidate : candidates) {
            if (Files.isReadable(candidate)) {
                return candidate;
            }
        }
        throw new IllegalStateException(
            "could not locate libtarwyn_ffi.so; set -Dtarwyn.library=/path/to/libtarwyn_ffi.so");
    }
}
