package org.tarwyn;

    /**
     * One control point of a bezier curve. `rotation_degrees` is absent for a point
     * that does not constrain heading.
     */
public final class Point {
    public final double x;
    public final double y;
    public final java.util.Optional<Double> rotationDegrees;

    public Point(double x, double y, java.util.Optional<Double> rotationDegrees) {
        this.x = x;
        this.y = y;
        this.rotationDegrees = rotationDegrees;
    }


    public double x() {
        return x;
    }

    public double y() {
        return y;
    }

    public java.util.Optional<Double> rotationDegrees() {
        return rotationDegrees;
    }


    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        Point other = (Point) value;
        return Double.compare(this.x, other.x) == 0 && Double.compare(this.y, other.y) == 0 && BoltFFIValueIdentity.optionalEquals(this.rotationDegrees, other.rotationDegrees, (leftValue0, rightValue0) -> Double.compare(leftValue0, rightValue0) == 0);
    }

    @Override
    public int hashCode() {
        int result = 1;
        result = 31 * result + Double.hashCode(this.x);
        result = 31 * result + Double.hashCode(this.y);
        result = 31 * result + BoltFFIValueIdentity.optionalHash(this.rotationDegrees, (itemValue0) -> Double.hashCode(itemValue0));
        return result;
    }

    @Override
    public String toString() {
        return "Point{" +
            "x=" + x +
            ", y=" + y +
            ", rotationDegrees=" + rotationDegrees
            + '}';
    }


    int wireSize() {
        return ((8 + 8) + WireSizes.optional(this.rotationDegrees, (__boltffi_value_0) -> 8));
    }

    void writeTo(WireWriter writer) {
        writer.writeDouble(this.x);
        writer.writeDouble(this.y);
        writer.writeOptional(this.rotationDegrees, (__boltffi_value_0) -> { writer.writeDouble(__boltffi_value_0); });
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

    static Point fromReader(WireReader reader) {
        return new Point(reader.readDouble(), reader.readDouble(), reader.readOptional(() -> reader.readDouble()));
    }

    static Point fromByteArray(byte[] bytes) {
        return fromReader(new WireReader(bytes));
    }
}