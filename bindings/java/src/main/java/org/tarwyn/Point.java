package org.tarwyn;

import java.util.OptionalDouble;

/** A control point of a bezier curve; the rotation is empty for a point with no heading. */
public record Point(double x, double y, OptionalDouble rotationDegrees) {
    /** A point with no heading. */
    public Point(double x, double y) {
        this(x, y, OptionalDouble.empty());
    }

    /** A point heading {@code rotationDegrees}. */
    public Point(double x, double y, double rotationDegrees) {
        this(x, y, OptionalDouble.of(rotationDegrees));
    }
}
