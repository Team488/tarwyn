package org.tarwyn;

    /**
     * A pose in space, with its rotation as a quaternion.
     * 
     * The field order is WPILib's `Pose3d` struct layout - a `Translation3d`
     * followed by a `Rotation3d`, which is a `Quaternion` written `w` first - so a
     * value written here reads back through WPILib's own deserialiser.
     * 
     * Rotation is a quaternion rather than roll, pitch and yaw because converting
     * between the two means committing to a rotation order, and getting that wrong
     * is silent. `Rotation3d` converts in both directions: construct one from
     * `roll`, `pitch`, `yaw` and read `getQuaternion()`, or take `getX()`, `getY()`
     * and `getZ()` back out.
     */
public final class Pose3d {
    public final double x;
    public final double y;
    public final double z;
    public final double qw;
    public final double qx;
    public final double qy;
    public final double qz;

    public Pose3d(double x, double y, double z, double qw, double qx, double qy, double qz) {
        this.x = x;
        this.y = y;
        this.z = z;
        this.qw = qw;
        this.qx = qx;
        this.qy = qy;
        this.qz = qz;
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

    public double qw() {
        return qw;
    }

    public double qx() {
        return qx;
    }

    public double qy() {
        return qy;
    }

    public double qz() {
        return qz;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        Pose3d other = (Pose3d) value;
        return Double.compare(this.x, other.x) == 0 && Double.compare(this.y, other.y) == 0 && Double.compare(this.z, other.z) == 0 && Double.compare(this.qw, other.qw) == 0 && Double.compare(this.qx, other.qx) == 0 && Double.compare(this.qy, other.qy) == 0 && Double.compare(this.qz, other.qz) == 0;
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + Double.hashCode(this.x);
        result = 31 * result + Double.hashCode(this.y);
        result = 31 * result + Double.hashCode(this.z);
        result = 31 * result + Double.hashCode(this.qw);
        result = 31 * result + Double.hashCode(this.qx);
        result = 31 * result + Double.hashCode(this.qy);
        result = 31 * result + Double.hashCode(this.qz);
        return result;
    }

    @Override
    public String toString() {
        return "Pose3d{" +
            "x=" + x +
            ", y=" + y +
            ", z=" + z +
            ", qw=" + qw +
            ", qx=" + qx +
            ", qy=" + qy +
            ", qz=" + qz
            + '}';
    }


    int wireSize() {
        return ((((((8 + 8) + 8) + 8) + 8) + 8) + 8);
    }

    void writeTo(WireWriter writer) {
        writer.writeDouble(this.x);
        writer.writeDouble(this.y);
        writer.writeDouble(this.z);
        writer.writeDouble(this.qw);
        writer.writeDouble(this.qx);
        writer.writeDouble(this.qy);
        writer.writeDouble(this.qz);
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
        return new Pose3d(reader.readDouble(), reader.readDouble(), reader.readDouble(), reader.readDouble(), reader.readDouble(), reader.readDouble(), reader.readDouble());
    }

    static final int STRUCT_SIZE = 56;


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
        buffer.putDouble((offset + 24), this.qw);
        buffer.putDouble((offset + 32), this.qx);
        buffer.putDouble((offset + 40), this.qy);
        buffer.putDouble((offset + 48), this.qz);
    }

    static Pose3d fromDirectBuffer(java.nio.ByteBuffer buffer, int offset) {
        return new Pose3d(buffer.getDouble((offset + 0)), buffer.getDouble((offset + 8)), buffer.getDouble((offset + 16)), buffer.getDouble((offset + 24)), buffer.getDouble((offset + 32)), buffer.getDouble((offset + 40)), buffer.getDouble((offset + 48)));
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