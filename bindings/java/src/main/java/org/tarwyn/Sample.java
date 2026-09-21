package org.tarwyn;

/** A value that arrived on a subscribed channel, as its protobuf {@code SupportedValues} encoding, or a log line. */
public record Sample(String channel, byte[] value) {
}
