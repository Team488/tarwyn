package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertTrue;

import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.Set;
import java.util.stream.Collectors;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

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
        "putBezierCurve", "putBezierCurves", "putBezierCurvesList", "putStruct",
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
        "delete", "deleteAll", "getTables", "getPing", "getServerStatistics",
        "getRawJson", "start", "stop", "close", "connect", "withPorts", "create",
    })
    void a_control_plane_call_exists(String name) {
        assertTrue(METHODS.contains(name), "missing " + name);
    }

    @ParameterizedTest(name = "{0}")
    @ValueSource(strings = {
        "compareAndSetAbsentString", "compareAndSetString", "compareAndSetDouble",
        "compareAndSetLong", "compareAndSetBoolean",
    })
    void a_compare_and_set_exists(String name) {
        assertTrue(METHODS.contains(name), "missing " + name);
    }

    @ParameterizedTest(name = "{0}")
    @ValueSource(strings = {
        "subscribe", "subscribeTelemetry", "subscribeToLogs",
        "unsubscribe", "unsubscribeTelemetry", "unsubscribeFromLogs",
        "publishTelemetry", "droppedPublishes", "droppedLogRecords",
        "logTo", "logToDrive", "loggingHealthy",
    })
    void a_subscription_or_logging_call_exists(String name) {
        assertTrue(METHODS.contains(name), "missing " + name);
    }
}
