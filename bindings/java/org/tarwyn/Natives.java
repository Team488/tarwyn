package org.tarwyn;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;

/** Unpacks the bundled native so the generated bindings can load it by path. */
final class Natives {
    private static final String[] OVERRIDES = {
        "uniffi.component.tarwyn.libraryOverride",
        "uniffi.component.tarwyn_types.libraryOverride",
    };

    private Natives() {
    }

    /**
     * Unpacks this platform's native beside a temporary file and points the
     * generated loader at it.
     *
     * <p>Does nothing when the property is already set or when the jar carries
     * no native, leaving the loader to search {@code java.library.path}.
     */
    static synchronized void install() {
        if (System.getProperty(OVERRIDES[0]) != null) {
            return;
        }
        String library = libraryName();
        String resource = "/" + platform() + "/" + library;
        try (InputStream bundled = Natives.class.getResourceAsStream(resource)) {
            if (bundled == null) {
                return;
            }
            Path directory = Files.createTempDirectory("tarwyn-native");
            Path unpacked = directory.resolve(library);
            Files.copy(bundled, unpacked, StandardCopyOption.REPLACE_EXISTING);
            unpacked.toFile().deleteOnExit();
            directory.toFile().deleteOnExit();
            String path = unpacked.toAbsolutePath().toString();
            for (String override : OVERRIDES) {
                System.setProperty(override, path);
            }
        } catch (IOException failure) {
            throw new IllegalStateException("could not unpack " + resource, failure);
        }
    }

    private static String platform() {
        String name = System.getProperty("os.name").toLowerCase(Locale.ROOT);
        String arch = System.getProperty("os.arch");
        String cpu = arch.equals("amd64") || arch.equals("x86_64") ? "x86_64"
            : arch.contains("aarch") ? "aarch64" : arch;
        if (name.contains("linux")) {
            return "linux-" + cpu;
        }
        if (name.contains("windows")) {
            return "windows-" + cpu;
        }
        if (name.contains("mac")) {
            return "darwin-" + (cpu.equals("aarch64") ? "arm64" : cpu);
        }
        throw new IllegalStateException("no bundled native for " + name + " " + arch);
    }

    private static String libraryName() {
        String name = System.getProperty("os.name").toLowerCase(Locale.ROOT);
        if (name.contains("windows")) {
            return "tarwyn_bindings.dll";
        }
        if (name.contains("mac")) {
            return "libtarwyn_bindings.dylib";
        }
        return "libtarwyn_bindings.so";
    }
}
