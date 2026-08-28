package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertTrue;

import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.Set;
import java.util.stream.Collectors;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/**
 * Pins the surface this client promises, one case per method.
 *
 * The claim is parity with the original TARWYN: every public put and get on its
 * Requests class exists here. A list asserted in one test would report that
 * something among fifty names went missing; a case per name reports which.
 */
final class ApiSurfaceTest {
    private static final Set<String> METHODS =
        Arrays.stream(TarwynClient.class.getMethods()).map(Method::getName).collect(
            Collectors.toUnmodifiableSet());

    @ParameterizedTest(name = "{0}")
    @ValueSource(strings = {
        "putString", "putInteger", "putLong", "putDouble", "putFloat", "putBoolean",
        "putStringList", "putBytesList", "putDoubleList", "putFloatList",
        "putIntegerList", "putLongList", "putBooleanList",
        "putPose2d", "putPose3d", "putCoordinates", "putBytes",
        "putUnknownBytes", "putTypedBytes",
        "putBezierCurve", "putBezierCurves", "putBezierCurvesList",
    })
    void a_publisher_exists(String name) {
        assertTrue(METHODS.contains(name), "missing " + name);
    }

    @ParameterizedTest(name = "{0}")
    @ValueSource(strings = {
        "getString", "getInteger", "getLong", "getDouble", "getFloat", "getBoolean",
        "getStringList", "getBytesList", "getDoubleList", "getFloatList",
        "getIntegerList", "getLongList", "getBooleanList",
        "getPose2d", "getPose3d", "getCoordinates", "getBytes", "getUnknownBytes",
        "getBezierCurve", "getBezierCurves", "getBezierCurvesList",
    })
    void a_reader_exists(String name) {
        assertTrue(METHODS.contains(name), "missing " + name);
    }

    @ParameterizedTest(name = "{0}")
    @ValueSource(strings = {
        "compareAndSetString", "compareAndSetInteger", "compareAndSetLong",
        "compareAndSetDouble", "compareAndSetFloat", "compareAndSetBoolean",
    })
    void a_compare_and_set_exists(String name) {
        assertTrue(METHODS.contains(name), "missing " + name);
    }

    @ParameterizedTest(name = "{0}")
    @ValueSource(strings = {
        "start", "close", "shutdown", "subscribe", "unsubscribe",
        "delete", "deleteAll", "getTables", "getPing", "getServerStatistics", "getRawJson",
        "logTo", "logToDrive", "droppedLogRecords", "loggingHealthy", "droppedPublishes",
    })
    void a_client_operation_exists(String name) {
        assertTrue(METHODS.contains(name), "missing " + name);
    }
}
