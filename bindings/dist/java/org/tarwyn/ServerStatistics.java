package org.tarwyn;

    /**
     * Server counters, as reported by [`TarwynClient::get_server_statistics`].
     */
public final class ServerStatistics {
    public final long channels;
    public final long values;
    public final long telemetrySubscribers;
    public final long uptimeSeconds;
    public final long droppedPublishes;
    public final long droppedLogs;
    public final String version;

    public ServerStatistics(long channels, long values, long telemetrySubscribers, long uptimeSeconds, long droppedPublishes, long droppedLogs, String version) {
        this.channels = channels;
        this.values = values;
        this.telemetrySubscribers = telemetrySubscribers;
        this.uptimeSeconds = uptimeSeconds;
        this.droppedPublishes = droppedPublishes;
        this.droppedLogs = droppedLogs;
        this.version = version;
    }


    public long channels() {
        return channels;
    }

    public long values() {
        return values;
    }

    public long telemetrySubscribers() {
        return telemetrySubscribers;
    }

    public long uptimeSeconds() {
        return uptimeSeconds;
    }

    public long droppedPublishes() {
        return droppedPublishes;
    }

    public long droppedLogs() {
        return droppedLogs;
    }

    public String version() {
        return version;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        ServerStatistics other = (ServerStatistics) value;
        return this.channels == other.channels && this.values == other.values && this.telemetrySubscribers == other.telemetrySubscribers && this.uptimeSeconds == other.uptimeSeconds && this.droppedPublishes == other.droppedPublishes && this.droppedLogs == other.droppedLogs && java.util.Objects.equals(this.version, other.version);
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + Long.hashCode(this.channels);
        result = 31 * result + Long.hashCode(this.values);
        result = 31 * result + Long.hashCode(this.telemetrySubscribers);
        result = 31 * result + Long.hashCode(this.uptimeSeconds);
        result = 31 * result + Long.hashCode(this.droppedPublishes);
        result = 31 * result + Long.hashCode(this.droppedLogs);
        result = 31 * result + java.util.Objects.hashCode(this.version);
        return result;
    }

    @Override
    public String toString() {
        return "ServerStatistics{" +
            "channels=" + channels +
            ", values=" + values +
            ", telemetrySubscribers=" + telemetrySubscribers +
            ", uptimeSeconds=" + uptimeSeconds +
            ", droppedPublishes=" + droppedPublishes +
            ", droppedLogs=" + droppedLogs +
            ", version=" + version
            + '}';
    }


    int wireSize() {
        return ((((((8 + 8) + 8) + 8) + 8) + 8) + WireSizes.string(this.version));
    }

    void writeTo(WireWriter writer) {
        writer.writeLong(this.channels);
        writer.writeLong(this.values);
        writer.writeLong(this.telemetrySubscribers);
        writer.writeLong(this.uptimeSeconds);
        writer.writeLong(this.droppedPublishes);
        writer.writeLong(this.droppedLogs);
        writer.writeString(this.version);
    }

    byte[] toByteArray() {
        WireLease lease = WireWriterPool.acquire(wireSize());
        try {
            writeTo(lease.writer());
            return lease.bytes();
        } finally {
            lease.close();
        }
    }

    static ServerStatistics fromReader(WireReader reader) {
        return new ServerStatistics(reader.readLong(), reader.readLong(), reader.readLong(), reader.readLong(), reader.readLong(), reader.readLong(), reader.readString());
    }

    static ServerStatistics fromByteArray(byte[] bytes) {
        return fromReader(new WireReader(bytes));
    }
}