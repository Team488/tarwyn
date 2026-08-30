package org.tarwyn;

    /**
     * A value published to a channel, delivered to a subscriber.
     * 
     * The payload is the encoded value; `channel` names what it arrived on, so one
     * subscription can carry several channels.
     */
public final class Update {
    public final String channel;
    public final byte[] value;

    public Update(String channel, byte[] value) {
        this.channel = channel;
        this.value = value;
    }


    public String channel() {
        return channel;
    }

    public byte[] value() {
        return value;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        Update other = (Update) value;
        return java.util.Objects.equals(this.channel, other.channel) && java.util.Arrays.equals(this.value, other.value);
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + java.util.Objects.hashCode(this.channel);
        result = 31 * result + java.util.Arrays.hashCode(this.value);
        return result;
    }

    @Override
    public String toString() {
        return "Update{" +
            "channel=" + channel +
            ", value=" + value
            + '}';
    }


    int wireSize() {
        return (WireSizes.string(this.channel) + (4 + this.value.length));
    }

    void writeTo(WireWriter writer) {
        writer.writeString(this.channel);
        writer.writeBytes(this.value);
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

    static Update fromReader(WireReader reader) {
        return new Update(reader.readString(), reader.readBytes());
    }

    static Update fromByteArray(byte[] bytes) {
        return fromReader(new WireReader(bytes));
    }
}