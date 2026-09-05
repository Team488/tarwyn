package org.tarwyn;

/** Receives the telemetry published to a channel this client subscribes to. */
@FunctionalInterface
public interface TelemetryUpdater {
    /** Called for each sample, on the client's receive thread. */
    void update(Telemetry telemetry);
}
