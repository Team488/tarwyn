package org.tarwyn;

    /**
     * A pose on the field plane.
     */
public final class Pose2d {
    public final double x;
    public final double y;
    public final double rotation;

    public Pose2d(double x, double y, double rotation) {
        this.x = x;
        this.y = y;
        this.rotation = rotation;
    }


    public double x() {
        return x;
    }

    public double y() {
        return y;
    }

    public double rotation() {
        return rotation;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        Pose2d other = (Pose2d) value;
        return Double.compare(this.x, other.x) == 0 && Double.compare(this.y, other.y) == 0 && Double.compare(this.rotation, other.rotation) == 0;
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + Double.hashCode(this.x);
        result = 31 * result + Double.hashCode(this.y);
        result = 31 * result + Double.hashCode(this.rotation);
        return result;
    }

    @Override
    public String toString() {
        return "Pose2d{" +
            "x=" + x +
            ", y=" + y +
            ", rotation=" + rotation
            + '}';
    }


    int wireSize() {
        return ((8 + 8) + 8);
    }

    void writeTo(WireWriter writer) {
        writer.writeDouble(this.x);
        writer.writeDouble(this.y);
        writer.writeDouble(this.rotation);
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

    static Pose2d fromReader(WireReader reader) {
        return new Pose2d(reader.readDouble(), reader.readDouble(), reader.readDouble());
    }

    static final int STRUCT_SIZE = 24;


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
        buffer.putDouble((offset + 16), this.rotation);
    }

    static Pose2d fromDirectBuffer(java.nio.ByteBuffer buffer, int offset) {
        return new Pose2d(buffer.getDouble((offset + 0)), buffer.getDouble((offset + 8)), buffer.getDouble((offset + 16)));
    }

    static Pose2d fromByteArray(byte[] bytes) {
        if (bytes.length != STRUCT_SIZE) {
            throw new IllegalArgumentException("invalid Pose2d byte size");
        }
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer
            .wrap(bytes)
            .order(java.nio.ByteOrder.nativeOrder());
        return fromDirectBuffer(buffer);
    }

    static Pose2d fromDirectBuffer(java.nio.ByteBuffer buffer) {
        return fromDirectBuffer(buffer, 0);
    }
}