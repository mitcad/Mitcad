# SPDX-License-Identifier: MIT
# A sketch of B-splines at the document's top, not attached, turned and
# moved: a cubic with its control points and end knots exposed, its first
# control point on the root point and a line joined to its end; a rational
# quadratic with a weight constraint on its middle control point; a cubic
# with an interior knot of multiplicity two (knot points exposed); a
# periodic cubic.

s = doc.addObject("Sketcher::SketchObject", "Splines")
s.Label = "Splines"
s.Placement = place((10, 20, 30), (1, 1, 1), 40)


def bspline(poles, mults, knots, degree, periodic=False, weights=None):
    curve = Part.BSplineCurve()
    vectors = [V(x, y, 0) for x, y in poles]
    if weights is None:
        curve.buildFromPolesMultsKnots(vectors, mults, knots, periodic, degree)
    else:
        curve.buildFromPolesMultsKnots(vectors, mults, knots, periodic, degree, weights)
    return curve


def control_circle(spline, at):
    """The control point circle of a spline's pole at (x, y)."""
    for c in s.Constraints:
        if c.Type != "InternalAlignment" or c.Second != spline:
            continue
        g = s.Geometry[c.First]
        if g.TypeId == "Part::GeomCircle" and (g.Center - V(at[0], at[1], 0)).Length < 1e-9:
            return c.First
    raise RuntimeError("no control point at %s" % (at,))


cubic = add(s, bspline([(0, 0), (10, 20), (30, 20), (40, 0)], [4, 4], [0.0, 1.0], 3))
s.exposeInternalGeometry(cubic)
constrain(s, "Coincident", control_circle(cubic, (0, 0)), MID, H_AXIS, START)
tail = add(s, line((40, 0), (60, 0)))
constrain(s, "Coincident", tail, START, cubic, END)
constrain(s, "Horizontal", tail)

w = math.sqrt(0.5)
rational = add(s, bspline([(0, 40), (20, 60), (40, 40)], [3, 3], [0.0, 1.0], 2, weights=[1.0, w, 1.0]))
s.exposeInternalGeometry(rational)
constrain(s, "Weight", control_circle(rational, (20, 60)), w)

kinked = add(
    s,
    bspline([(50, 40), (55, 55), (65, 55), (70, 40), (80, 30), (90, 40)], [4, 2, 4], [0.0, 0.5, 1.0], 3),
)
s.exposeInternalGeometry(kinked)

ring = [(110 + 10 * math.cos(a), 30 + 10 * math.sin(a)) for a in [i * math.pi / 3 for i in range(6)]]
add(s, bspline(ring, [1] * 7, [i / 6.0 for i in range(7)], 3, periodic=True))
