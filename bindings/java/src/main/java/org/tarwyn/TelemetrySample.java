package org.tarwyn;

/** A telemetry sample and the publisher's timestamp in microseconds since the Unix epoch. */
public record TelemetrySample(long timestampMicros, byte[] payload) {
}
