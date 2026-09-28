package org.tarwyn;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BOOLEAN;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_CHAR;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_FLOAT;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.StructLayout;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * The native library: one method handle per function and the three upcall
 * stubs every subscription shares.
 *
 * <p>The library is taken from the path in the {@value #LIBRARY_PROPERTY}
 * system property, else from the library path, else unpacked from the jar.
 */
final class Native {
    static final String LIBRARY_PROPERTY = "tarwyn.library";
    static final int ABI_VERSION = 2;

    static final StructLayout STATISTICS = MemoryLayout.structLayout(
        JAVA_LONG.withName("channels"),
        JAVA_LONG.withName("values"),
        JAVA_LONG.withName("telemetry_subscribers"),
        JAVA_LONG.withName("uptime_seconds"),
        JAVA_LONG.withName("dropped_publishes"),
        JAVA_LONG.withName("dropped_logs"));

    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup LOOKUP = lookup();

    static final MethodHandle ABI = handle("tarwyn_abi_version", FunctionDescriptor.of(JAVA_INT));
    static final MethodHandle TAKE_LAST_ERROR =
        handle("tarwyn_take_last_error", FunctionDescriptor.of(ADDRESS, ADDRESS));
    static final MethodHandle NEW = handle("tarwyn_client_new", FunctionDescriptor.of(ADDRESS));
    static final MethodHandle CONNECT =
        handle("tarwyn_client_connect", FunctionDescriptor.of(ADDRESS, ADDRESS, JAVA_LONG));
    /** The two ports are C {@code uint16_t}. {@code char} is Java's unsigned 16-bit carrier, so the linker zero-extends. */
    static final MethodHandle WITH_PORTS = handle("tarwyn_client_with_ports", FunctionDescriptor.of(
        ADDRESS, ADDRESS, JAVA_LONG, JAVA_CHAR, JAVA_CHAR, JAVA_LONG, JAVA_INT, JAVA_LONG, JAVA_LONG));
    static final MethodHandle DEFAULT_PREDICT_MICROS =
        handle("tarwyn_default_predict_micros", FunctionDescriptor.of(JAVA_LONG));
    static final MethodHandle FREE = handle("tarwyn_client_free", FunctionDescriptor.ofVoid(ADDRESS));
    static final MethodHandle START = handle("tarwyn_client_start", FunctionDescriptor.ofVoid(ADDRESS));
    static final MethodHandle STOP = handle("tarwyn_client_stop", FunctionDescriptor.ofVoid(ADDRESS));
    static final MethodHandle BYTES_FREE =
        handle("tarwyn_bytes_free", FunctionDescriptor.ofVoid(ADDRESS, JAVA_LONG));

    static final MethodHandle PUT_STRING = put("tarwyn_put_string", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_INTEGER = put("tarwyn_put_integer", JAVA_INT);
    static final MethodHandle PUT_LONG = put("tarwyn_put_long", JAVA_LONG);
    static final MethodHandle PUT_DOUBLE = put("tarwyn_put_double", JAVA_DOUBLE);
    static final MethodHandle PUT_FLOAT = put("tarwyn_put_float", JAVA_FLOAT);
    static final MethodHandle PUT_BOOLEAN = put("tarwyn_put_boolean", JAVA_BOOLEAN);
    static final MethodHandle PUT_BYTES = put("tarwyn_put_bytes", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_STRING_LIST = put("tarwyn_put_string_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_BYTES_LIST = put("tarwyn_put_bytes_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_DOUBLE_LIST = put("tarwyn_put_double_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_FLOAT_LIST = put("tarwyn_put_float_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_INTEGER_LIST = put("tarwyn_put_integer_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_LONG_LIST = put("tarwyn_put_long_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_BOOLEAN_LIST = put("tarwyn_put_boolean_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_COORDINATES = put("tarwyn_put_coordinates", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_POSE2D = put("tarwyn_put_pose2d", JAVA_DOUBLE, JAVA_DOUBLE, JAVA_DOUBLE);
    static final MethodHandle PUT_POSE3D = put("tarwyn_put_pose3d",
        JAVA_DOUBLE, JAVA_DOUBLE, JAVA_DOUBLE, JAVA_DOUBLE, JAVA_DOUBLE, JAVA_DOUBLE, JAVA_DOUBLE);
    static final MethodHandle PUT_BEZIER_CURVE = put("tarwyn_put_bezier_curve", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_BEZIER_CURVES = putBool("tarwyn_put_bezier_curves", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_BEZIER_CURVES_LIST =
        putBool("tarwyn_put_bezier_curves_list", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_TYPED_BYTES = putBool("tarwyn_put_typed_bytes", JAVA_INT, ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_UNKNOWN_BYTES = put("tarwyn_put_unknown_bytes", ADDRESS, JAVA_LONG);
    static final MethodHandle PUT_STRUCT =
        put("tarwyn_put_struct", ADDRESS, JAVA_LONG, ADDRESS, JAVA_LONG, ADDRESS, JAVA_LONG);

    static final MethodHandle GET_STRING = read("tarwyn_get_string");
    static final MethodHandle GET_INTEGER = getScalar("tarwyn_get_integer");
    static final MethodHandle GET_LONG = getScalar("tarwyn_get_long");
    static final MethodHandle GET_DOUBLE = getScalar("tarwyn_get_double");
    static final MethodHandle GET_FLOAT = getScalar("tarwyn_get_float");
    static final MethodHandle GET_BOOLEAN = getScalar("tarwyn_get_boolean");
    static final MethodHandle GET_BYTES = read("tarwyn_get_bytes");
    static final MethodHandle GET_STRING_LIST = read("tarwyn_get_string_list");
    static final MethodHandle GET_BYTES_LIST = read("tarwyn_get_bytes_list");
    static final MethodHandle GET_DOUBLE_LIST = read("tarwyn_get_double_list");
    static final MethodHandle GET_FLOAT_LIST = read("tarwyn_get_float_list");
    static final MethodHandle GET_INTEGER_LIST = read("tarwyn_get_integer_list");
    static final MethodHandle GET_LONG_LIST = read("tarwyn_get_long_list");
    static final MethodHandle GET_BOOLEAN_LIST = read("tarwyn_get_boolean_list");
    static final MethodHandle GET_COORDINATES = read("tarwyn_get_coordinates");
    static final MethodHandle GET_BEZIER_CURVE = read("tarwyn_get_bezier_curve");
    static final MethodHandle GET_BEZIER_CURVES = read("tarwyn_get_bezier_curves");
    static final MethodHandle GET_BEZIER_CURVES_LIST = read("tarwyn_get_bezier_curves_list");
    static final MethodHandle GET_POSE2D = getScalar("tarwyn_get_pose2d");
    static final MethodHandle GET_POSE3D = getScalar("tarwyn_get_pose3d");
    static final MethodHandle GET_UNKNOWN_BYTES = read("tarwyn_get_unknown_bytes");

    static final MethodHandle DELETE =
        handle("tarwyn_delete", FunctionDescriptor.of(JAVA_INT, ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle DELETE_ALL = handle("tarwyn_delete_all", FunctionDescriptor.of(JAVA_INT, ADDRESS));
    static final MethodHandle GET_TABLES = read("tarwyn_get_tables");
    static final MethodHandle GET_PING =
        handle("tarwyn_get_ping", FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS));
    static final MethodHandle GET_SERVER_STATISTICS = handle("tarwyn_get_server_statistics",
        FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, ADDRESS, ADDRESS));
    static final MethodHandle GET_RAW_JSON = read("tarwyn_get_raw_json");
    static final MethodHandle CAS_ABSENT_STRING =
        putBool("tarwyn_compare_and_set_absent_string", ADDRESS, JAVA_LONG);
    static final MethodHandle CAS_STRING =
        putBool("tarwyn_compare_and_set_string", ADDRESS, JAVA_LONG, ADDRESS, JAVA_LONG);
    static final MethodHandle CAS_DOUBLE = putBool("tarwyn_compare_and_set_double", JAVA_DOUBLE, JAVA_DOUBLE);
    static final MethodHandle CAS_LONG = putBool("tarwyn_compare_and_set_long", JAVA_LONG, JAVA_LONG);
    static final MethodHandle CAS_BOOLEAN =
        putBool("tarwyn_compare_and_set_boolean", JAVA_BOOLEAN, JAVA_BOOLEAN);

    static final MethodHandle PUBLISH_TELEMETRY = put("tarwyn_publish_telemetry", ADDRESS, JAVA_LONG);
    static final MethodHandle LOG_TO =
        handle("tarwyn_log_to", FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle LOG_TO_DRIVE = read("tarwyn_log_to_drive");
    static final MethodHandle DROPPED_LOG_RECORDS =
        handle("tarwyn_dropped_log_records", FunctionDescriptor.of(JAVA_LONG, ADDRESS));
    static final MethodHandle LOGGING_HEALTHY =
        handle("tarwyn_logging_healthy", FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS));
    static final MethodHandle DROPPED_PUBLISHES =
        handle("tarwyn_dropped_publishes", FunctionDescriptor.of(JAVA_LONG, ADDRESS));

    static final MethodHandle SUBSCRIBE = handle("tarwyn_subscribe",
        FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, JAVA_LONG, ADDRESS, ADDRESS, ADDRESS));
    static final MethodHandle UNSUBSCRIBE =
        handle("tarwyn_unsubscribe", FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle SUBSCRIBE_TELEMETRY = handle("tarwyn_subscribe_telemetry",
        FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, JAVA_LONG, ADDRESS, ADDRESS, ADDRESS));
    static final MethodHandle UNSUBSCRIBE_TELEMETRY =
        handle("tarwyn_unsubscribe_telemetry", FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, JAVA_LONG));
    static final MethodHandle SUBSCRIBE_TO_LOGS = handle("tarwyn_subscribe_to_logs",
        FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, ADDRESS, ADDRESS));
    static final MethodHandle UNSUBSCRIBE_FROM_LOGS =
        handle("tarwyn_unsubscribe_from_logs", FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS));

    /** Callbacks by the context handed to the library, released when its drop upcall arrives. */
    private static final Map<Long, Consumer<Sample>> SAMPLE_CALLBACKS = new ConcurrentHashMap<>();
    private static final Map<Long, Consumer<TelemetrySample>> TELEMETRY_CALLBACKS = new ConcurrentHashMap<>();
    private static final AtomicLong NEXT_CONTEXT = new AtomicLong(1);

    static final MemorySegment ON_SAMPLE;
    static final MemorySegment ON_TELEMETRY;
    static final MemorySegment ON_DROP;

    static {
        try {
            MethodHandles.Lookup self = MethodHandles.lookup();
            ON_SAMPLE = LINKER.upcallStub(
                self.findStatic(Native.class, "onSample", MethodType.methodType(
                    void.class, MemorySegment.class, MemorySegment.class, long.class, MemorySegment.class, long.class)),
                FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, JAVA_LONG, ADDRESS, JAVA_LONG),
                Arena.global());
            ON_TELEMETRY = LINKER.upcallStub(
                self.findStatic(Native.class, "onTelemetrySample", MethodType.methodType(
                    void.class, MemorySegment.class, long.class, MemorySegment.class, long.class)),
                FunctionDescriptor.ofVoid(ADDRESS, JAVA_LONG, ADDRESS, JAVA_LONG),
                Arena.global());
            ON_DROP = LINKER.upcallStub(
                self.findStatic(Native.class, "onDrop", MethodType.methodType(void.class, MemorySegment.class)),
                FunctionDescriptor.ofVoid(ADDRESS),
                Arena.global());
        } catch (ReflectiveOperationException failure) {
            throw new ExceptionInInitializerError(failure);
        }
        int abi;
        try {
            abi = (int) ABI.invokeExact();
        } catch (Throwable failure) {
            throw new ExceptionInInitializerError(failure);
        }
        if (abi != ABI_VERSION) {
            throw new ExceptionInInitializerError(
                "the tarwyn library speaks ABI " + abi + ", this client ABI " + ABI_VERSION);
        }
    }

    private Native() {
    }

    private static SymbolLookup lookup() {
        String override = System.getProperty(LIBRARY_PROPERTY);
        if (override != null) {
            return SymbolLookup.libraryLookup(Path.of(override), Arena.global());
        }
        try {
            return SymbolLookup.libraryLookup(System.mapLibraryName("tarwyn"), Arena.global());
        } catch (IllegalArgumentException notOnLibraryPath) {
            Path unpacked = unpack();
            if (unpacked == null) {
                throw notOnLibraryPath;
            }
            System.setProperty(LIBRARY_PROPERTY, unpacked.toString());
            return SymbolLookup.libraryLookup(unpacked, Arena.global());
        }
    }

    private static Path unpack() {
        String library = System.mapLibraryName("tarwyn");
        String resource = "/" + platform() + "/" + library;
        try (InputStream bundled = Native.class.getResourceAsStream(resource)) {
            if (bundled == null) {
                return null;
            }
            Path directory = Files.createTempDirectory("tarwyn-native");
            Path unpacked = directory.resolve(library);
            Files.copy(bundled, unpacked, StandardCopyOption.REPLACE_EXISTING);
            unpacked.toFile().deleteOnExit();
            directory.toFile().deleteOnExit();
            return unpacked.toAbsolutePath();
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

    private static MethodHandle handle(String name, FunctionDescriptor descriptor) {
        return LINKER.downcallHandle(LOOKUP.findOrThrow(name), descriptor);
    }

    private static MethodHandle put(String name, MemoryLayout... value) {
        return handle(name, FunctionDescriptor.ofVoid(prepend(value)));
    }

    private static MethodHandle putBool(String name, MemoryLayout... value) {
        return handle(name, FunctionDescriptor.of(JAVA_BOOLEAN, prepend(value)));
    }

    private static MethodHandle read(String name) {
        return handle(name, FunctionDescriptor.of(ADDRESS, ADDRESS, ADDRESS, JAVA_LONG, ADDRESS));
    }

    private static MethodHandle getScalar(String name) {
        return handle(name, FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, JAVA_LONG, ADDRESS));
    }

    private static MemoryLayout[] prepend(MemoryLayout[] value) {
        MemoryLayout[] layouts = new MemoryLayout[value.length + 3];
        layouts[0] = ADDRESS;
        layouts[1] = ADDRESS;
        layouts[2] = JAVA_LONG;
        System.arraycopy(value, 0, layouts, 3, value.length);
        return layouts;
    }

    static MemorySegment register(Consumer<Sample> callback) {
        long context = NEXT_CONTEXT.getAndIncrement();
        SAMPLE_CALLBACKS.put(context, callback);
        return MemorySegment.ofAddress(context);
    }

    static MemorySegment registerTelemetry(Consumer<TelemetrySample> callback) {
        long context = NEXT_CONTEXT.getAndIncrement();
        TELEMETRY_CALLBACKS.put(context, callback);
        return MemorySegment.ofAddress(context);
    }

    static byte[] bytes(MemorySegment pointer, long length) {
        return pointer.reinterpret(length).toArray(JAVA_BYTE);
    }

    static String text(MemorySegment pointer, long length) {
        return new String(bytes(pointer, length), StandardCharsets.UTF_8);
    }

    private static void onSample(
        MemorySegment context, MemorySegment channel, long channelLength, MemorySegment value, long valueLength) {
        Consumer<Sample> callback = SAMPLE_CALLBACKS.get(context.address());
        if (callback != null) {
            deliver(() -> callback.accept(new Sample(text(channel, channelLength), bytes(value, valueLength))));
        }
    }

    private static void onTelemetrySample(
        MemorySegment context, long timestampMicros, MemorySegment payload, long payloadLength) {
        Consumer<TelemetrySample> callback = TELEMETRY_CALLBACKS.get(context.address());
        if (callback != null) {
            deliver(() -> callback.accept(new TelemetrySample(timestampMicros, bytes(payload, payloadLength))));
        }
    }

    private static void onDrop(MemorySegment context) {
        SAMPLE_CALLBACKS.remove(context.address());
        TELEMETRY_CALLBACKS.remove(context.address());
    }

    private static void deliver(Runnable callback) {
        try {
            callback.run();
        } catch (Throwable failure) {
            Thread thread = Thread.currentThread();
            Thread.UncaughtExceptionHandler handler = thread.getUncaughtExceptionHandler();
            if (handler != null) {
                handler.uncaughtException(thread, failure);
            }
        }
    }
}
