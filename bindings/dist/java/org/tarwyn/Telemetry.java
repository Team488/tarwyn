package org.tarwyn;

    /**
     * A telemetry datagram, with the publisher's clock.
     */
public final class Telemetry {
    public final long timestampMicros;
    public final byte[] payload;

    public Telemetry(long timestampMicros, byte[] payload) {
        this.timestampMicros = timestampMicros;
        this.payload = payload;
    }


    public long timestampMicros() {
        return timestampMicros;
    }

    public byte[] payload() {
        return payload;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        Telemetry other = (Telemetry) value;
        return this.timestampMicros == other.timestampMicros && java.util.Arrays.equals(this.payload, other.payload);
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + Long.hashCode(this.timestampMicros);
        result = 31 * result + java.util.Arrays.hashCode(this.payload);
        return result;
    }

    @Override
    public String toString() {
        return "Telemetry{" +
            "timestampMicros=" + timestampMicros +
            ", payload=" + payload
            + '}';
    }


    int wireSize() {
        return (8 + (4 + this.payload.length));
    }

    void writeTo(WireWriter writer) {
        writer.writeLong(this.timestampMicros);
        writer.writeBytes(this.payload);
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

    static Telemetry fromReader(WireReader reader) {
        return new Telemetry(reader.readLong(), reader.readBytes());
    }

    static Telemetry fromByteArray(byte[] bytes) {
        return fromReader(new WireReader(bytes));
    }
}