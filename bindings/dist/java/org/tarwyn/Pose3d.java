package org.tarwyn;

    /**
     * A pose in space.
     */
public final class Pose3d {
    public final double x;
    public final double y;
    public final double z;
    public final double roll;
    public final double pitch;
    public final double yaw;

    public Pose3d(double x, double y, double z, double roll, double pitch, double yaw) {
        this.x = x;
        this.y = y;
        this.z = z;
        this.roll = roll;
        this.pitch = pitch;
        this.yaw = yaw;
    }


    public double x() {
        return x;
    }

    public double y() {
        return y;
    }

    public double z() {
        return z;
    }

    public double roll() {
        return roll;
    }

    public double pitch() {
        return pitch;
    }

    public double yaw() {
        return yaw;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        Pose3d other = (Pose3d) value;
        return Double.compare(this.x, other.x) == 0 && Double.compare(this.y, other.y) == 0 && Double.compare(this.z, other.z) == 0 && Double.compare(this.roll, other.roll) == 0 && Double.compare(this.pitch, other.pitch) == 0 && Double.compare(this.yaw, other.yaw) == 0;
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + Double.hashCode(this.x);
        result = 31 * result + Double.hashCode(this.y);
        result = 31 * result + Double.hashCode(this.z);
        result = 31 * result + Double.hashCode(this.roll);
        result = 31 * result + Double.hashCode(this.pitch);
        result = 31 * result + Double.hashCode(this.yaw);
        return result;
    }

    @Override
    public String toString() {
        return "Pose3d{" +
            "x=" + x +
            ", y=" + y +
            ", z=" + z +
            ", roll=" + roll +
            ", pitch=" + pitch +
            ", yaw=" + yaw
            + '}';
    }


    int wireSize() {
        return (((((8 + 8) + 8) + 8) + 8) + 8);
    }

    void writeTo(WireWriter writer) {
        writer.writeDouble(this.x);
        writer.writeDouble(this.y);
        writer.writeDouble(this.z);
        writer.writeDouble(this.roll);
        writer.writeDouble(this.pitch);
        writer.writeDouble(this.yaw);
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

    static Pose3d fromReader(WireReader reader) {
        return new Pose3d(reader.readDouble(), reader.readDouble(), reader.readDouble(), reader.readDouble(), reader.readDouble(), reader.readDouble());
    }

    static final int STRUCT_SIZE = 48;


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
        buffer.putDouble((offset + 16), this.z);
        buffer.putDouble((offset + 24), this.roll);
        buffer.putDouble((offset + 32), this.pitch);
        buffer.putDouble((offset + 40), this.yaw);
    }

    static Pose3d fromDirectBuffer(java.nio.ByteBuffer buffer, int offset) {
        return new Pose3d(buffer.getDouble((offset + 0)), buffer.getDouble((offset + 8)), buffer.getDouble((offset + 16)), buffer.getDouble((offset + 24)), buffer.getDouble((offset + 32)), buffer.getDouble((offset + 40)));
    }

    static Pose3d fromByteArray(byte[] bytes) {
        if (bytes.length != STRUCT_SIZE) {
            throw new IllegalArgumentException("invalid Pose3d byte size");
        }
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer
            .wrap(bytes)
            .order(java.nio.ByteOrder.nativeOrder());
        return fromDirectBuffer(buffer);
    }

    static Pose3d fromDirectBuffer(java.nio.ByteBuffer buffer) {
        return fromDirectBuffer(buffer, 0);
    }
}