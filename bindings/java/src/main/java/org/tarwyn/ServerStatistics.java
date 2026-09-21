package org.tarwyn;

/** What the server reports about itself. */
public record ServerStatistics(
    long channels,
    long values,
    long telemetrySubscribers,
    long uptimeSeconds,
    long droppedPublishes,
    long droppedLogs,
    String version) {
}
