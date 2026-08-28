package org.tarwyn;

import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.List;
import java.util.concurrent.CompletableFuture;

/**
 * Builds an {@link TarwynClient} on a background thread.
 *
 * Constructing a client loads the native library and binds sockets, which is slow
 * enough to matter during robot init. This starts that work and hands back a
 * handle to poll or await, so init is not blocked by it.
 */
public final class TarwynClientManager {
    private final CompletableFuture<TarwynClient> future;

    private TarwynClientManager(CompletableFuture<TarwynClient> future) {
        this.future = future;
    }

    /**
     * Begin connecting to a server on localhost.
     *
     * @return a handle to the client being built
     */
    public static TarwynClientManager getDefaultClientAsynchronously() {
        return getClientAsynchronously("127.0.0.1");
    }

    /**
     * Begin connecting to a server on {@code host}, with the bundled native library.
     *
     * @param host the machine running the server
     * @return a handle to the client being built
     */
    public static TarwynClientManager getClientAsynchronously(String host) {
        return getClientAsynchronously(host, defaultLibrary());
    }

    /**
     * Begin connecting to a server on {@code host}, loading the native library from
     * {@code library}.
     *
     * @param host the machine running the server
     * @param library the native library to load
     * @return a handle to the client being built
     */
    public static TarwynClientManager getClientAsynchronously(String host, Path library) {
        return new TarwynClientManager(
            CompletableFuture.supplyAsync(() -> new TarwynClient(library, host)));
    }

    /**
     * The future the client will complete on. Completes exceptionally if the client
     * could not be built.
     *
     * @return the future
     */
    public CompletableFuture<TarwynClient> getClientFuture() {
        return future;
    }

    /**
     * The client if it is ready, without waiting.
     *
     * Read from the future rather than mirrored into a field, so it cannot lag
     * behind a future that has already completed.
     *
     * @return the client, or null while it is still being built or if building failed
     */
    public TarwynClient getOrNull() {
        return future.isDone() && !future.isCompletedExceptionally()
            ? future.getNow(null)
            : null;
    }

    /**
     * Whether the client has finished being built.
     *
     * @return true once it is ready
     * @throws IllegalStateException if building the client failed; polling forever
     *     for a client that will never arrive is worse than being told
     */
    public boolean isReady() {
        Throwable failure = failure();
        if (failure != null) {
            throw new IllegalStateException("the client could not be built", failure);
        }
        return getOrNull() != null;
    }

    /**
     * Why building the client failed, if it did.
     *
     * Checked by {@link #isReady()}, which throws rather than reporting false
     * forever. Call this directly to inspect the failure without one.
     *
     * @return the failure, or null while it is still building or once it succeeded
     */
    public Throwable failure() {
        if (!future.isCompletedExceptionally()) {
            return null;
        }
        try {
            future.getNow(null);
            return null;
        } catch (java.util.concurrent.CompletionException error) {
            return error.getCause() != null ? error.getCause() : error;
        } catch (RuntimeException error) {
            return error;
        }
    }

    /**
     * Close the client if it was built. Does nothing while it is still being built,
     * so a client that finishes after this call is not closed by it.
     */
    public void shutdown() {
        TarwynClient existing = getOrNull();
        if (existing != null) {
            existing.close();
        }
    }

    static Path defaultLibrary() {
        String override = System.getProperty("tarwyn.library");
        if (override != null) {
            return Path.of(override);
        }
        Path packaged = extractPackagedLibrary();
        if (packaged != null) {
            return packaged;
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
            "could not locate " + libraryName() + " on disk or in the jar; "
                + "set -Dtarwyn.library=/path/to/" + libraryName());
    }

    static String platform() {
        String os = System.getProperty("os.name").toLowerCase();
        String raw = System.getProperty("os.arch").toLowerCase();
        String arch = raw.equals("amd64") || raw.equals("x86_64") ? "x86_64"
            : raw.contains("aarch") || raw.equals("arm64") ? "aarch64" : raw;
        if (os.contains("win")) {
            return "windows-" + arch;
        }
        if (os.contains("mac") || os.contains("darwin")) {
            return "macos-" + arch;
        }
        return "linux-" + arch;
    }

    static String libraryName() {
        String os = System.getProperty("os.name").toLowerCase();
        if (os.contains("win")) {
            return "tarwyn_ffi.dll";
        }
        return os.contains("mac") || os.contains("darwin")
            ? "libtarwyn_ffi.dylib" : "libtarwyn_ffi.so";
    }

    private static Path extractPackagedLibrary() {
        String resource = "/natives/" + platform() + "/" + libraryName();
        byte[] library;
        try (InputStream stream = TarwynClientManager.class.getResourceAsStream(resource)) {
            if (stream == null) {
                return null;
            }
            library = stream.readAllBytes();
        } catch (Exception error) {
            return null;
        }

        String expected = readChecksum(resource + ".sha256");
        if (expected != null && !expected.equals(sha256(library))) {
            throw new IllegalStateException(
                "the packaged " + libraryName() + " does not match its recorded checksum");
        }

        try {
            Path directory = Files.createTempDirectory("tarwyn-native");
            directory.toFile().deleteOnExit();
            Path target = directory.resolve(libraryName());
            Files.copy(new java.io.ByteArrayInputStream(library), target,
                StandardCopyOption.REPLACE_EXISTING);
            target.toFile().deleteOnExit();
            return target;
        } catch (Exception error) {
            throw new IllegalStateException("could not unpack " + libraryName(), error);
        }
    }

    private static String readChecksum(String resource) {
        try (InputStream stream = TarwynClientManager.class.getResourceAsStream(resource)) {
            return stream == null ? null : new String(stream.readAllBytes()).trim();
        } catch (Exception error) {
            return null;
        }
    }

    private static String sha256(byte[] data) {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data));
        } catch (Exception error) {
            throw new IllegalStateException("SHA-256 is unavailable", error);
        }
    }
}
