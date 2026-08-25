import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.file.Path;

final class TarwynNative {
    static final int XT_OK = 0;
    static final int XT_ERR_NULL = -1;
    static final int XT_ERR_UTF8 = -2;
    static final int XT_ERR_NO_VALUE = -3;
    static final int XT_ERR_WRONG_TYPE = -4;
    static final int XT_ERR_PANIC = -5;

    private static final Linker LINKER = Linker.nativeLinker();
    private final SymbolLookup lookup;

    final MethodHandle clientNew;
    final MethodHandle clientStart;
    final MethodHandle clientFree;
    final MethodHandle droppedPublishes;
    final MethodHandle publishDouble;
    final MethodHandle publishBool;
    final MethodHandle publishString;
    final MethodHandle publishBytes;
    final MethodHandle getDouble;
    final MethodHandle subscribeRing;
    final MethodHandle unsubscribe;
    final MethodHandle ringBase;
    final MethodHandle ringWriteIndex;

    TarwynNative(Path library, Arena arena) {
        this.lookup = SymbolLookup.libraryLookup(library, arena);

        clientNew = bind("xt_client_new", FunctionDescriptor.of(
            ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_SHORT,
            ValueLayout.JAVA_SHORT, ValueLayout.JAVA_SHORT, ValueLayout.JAVA_LONG,
            ValueLayout.JAVA_INT));
        clientStart = bind("xt_client_start",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS));
        clientFree = bind("xt_client_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));
        droppedPublishes = bind("xt_dropped_publishes",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        publishDouble = bind("xt_publish_double", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_DOUBLE));
        publishBool = bind("xt_publish_bool", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_BOOLEAN));
        publishString = bind("xt_publish_string", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        publishBytes = bind("xt_publish_bytes", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS,
            ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
        getDouble = bind("xt_get_double", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        subscribeRing = bind("xt_subscribe_ring", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));
        unsubscribe = bind("xt_unsubscribe", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
        ringBase = bind("xt_ring_base", FunctionDescriptor.of(
            ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
        ringWriteIndex = bind("xt_ring_write_index", FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));
    }

    private MethodHandle bind(String name, FunctionDescriptor descriptor) {
        MemorySegment symbol = lookup.find(name).orElseThrow(
            () -> new UnsatisfiedLinkError("symbol not found in the tarwyn library: " + name));
        return LINKER.downcallHandle(symbol, descriptor);
    }

    static String describe(int code) {
        return switch (code) {
            case XT_OK -> "ok";
            case XT_ERR_NULL -> "null pointer or poisoned lock";
            case XT_ERR_UTF8 -> "argument was not valid UTF-8";
            case XT_ERR_NO_VALUE -> "no value for that channel, or no such subscription";
            case XT_ERR_WRONG_TYPE -> "channel holds a different type";
            case XT_ERR_PANIC -> "a panic was caught at the boundary";
            default -> "unknown code " + code;
        };
    }
}
