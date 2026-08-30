package org.tarwyn;

    /**
     * An `(x, y)` pair, as carried by the coordinate list type.
     */
public final class Coordinate {
    public final double x;
    public final double y;

    public Coordinate(double x, double y) {
        this.x = x;
        this.y = y;
    }


    public double x() {
        return x;
    }

    public double y() {
        return y;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        Coordinate other = (Coordinate) value;
        return Double.compare(this.x, other.x) == 0 && Double.compare(this.y, other.y) == 0;
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + Double.hashCode(this.x);
        result = 31 * result + Double.hashCode(this.y);
        return result;
    }

    @Override
    public String toString() {
        return "Coordinate{" +
            "x=" + x +
            ", y=" + y
            + '}';
    }


    int wireSize() {
        return (8 + 8);
    }

    void writeTo(WireWriter writer) {
        writer.writeDouble(this.x);
        writer.writeDouble(this.y);
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

    static Coordinate fromReader(WireReader reader) {
        return new Coordinate(reader.readDouble(), reader.readDouble());
    }

    static final int STRUCT_SIZE = 16;


    java.nio.ByteBuffer toDirectBuffer() {
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer
            .allocateDirect(STRUCT_SIZE)
            .order(java.nio.ByteOrder.nativeOrder());
        writeToDirectBuffer(buffer, 0);
        return buffer;
    }

    void writeToDirectBuffer(java.nio.ByteBuffer buffer, int offset) {
        buffer.putDouble((offset + 0), this.x);
        buffer.putDouble((offset + 8), this.y);
    }

    static Coordinate fromDirectBuffer(java.nio.ByteBuffer buffer, int offset) {
        return new Coordinate(buffer.getDouble((offset + 0)), buffer.getDouble((offset + 8)));
    }

    static Coordinate fromByteArray(byte[] bytes) {
        if (bytes.length != STRUCT_SIZE) {
            throw new IllegalArgumentException("invalid Coordinate byte size");
        }
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer
            .wrap(bytes)
            .order(java.nio.ByteOrder.nativeOrder());
        return fromDirectBuffer(buffer);
    }

    static Coordinate fromDirectBuffer(java.nio.ByteBuffer buffer) {
        return fromDirectBuffer(buffer, 0);
    }
}