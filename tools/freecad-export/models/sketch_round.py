# SPDX-License-Identifier: MIT
# A sketch of circles and arcs in a Body, on its XZ plane: radius and
# diameter, a centre on the root point and on the H axis, the distance of
# two centres, equal circles, centres in a vertical line, a line tangent to
# a circle, a line tangent to an arc end to end and arcs tangent end to
# end, a line's end tangent to a circle, a line perpendicular to a circle,
# a point on a circle, concentric circles (coincident centres), an arc's
# angle, an arc made with its normal along -Z, and from FreeCAD 1.0 an
# arc's length.

b = body("RoundBody")
s = sketch_on(b, "Round", plane="XZ_Plane")

big = add(s, circle((0, 0), 10))
constrain(s, "Radius", big, 10.0)
constrain(s, "Coincident", big, MID, H_AXIS, START)
small = add(s, circle((30, 0), 5))
constrain(s, "Diameter", small, 10.0, name="hole")
constrain(s, "PointOnObject", small, MID, H_AXIS)
constrain(s, "Distance", big, MID, small, MID, 30.0)
other = add(s, circle((30, 20), 5))
constrain(s, "Equal", small, other)
constrain(s, "Vertical", small, MID, other, MID)

top = add(s, line((-20, 10), (20, 10)))
constrain(s, "Horizontal", top)
constrain(s, "Tangent", top, big)

first = add(s, arc((60, 0), 8, 0, 90))
constrain(s, "Radius", first, 8.0)
riser = add(s, line((68, -20), (68, 0)))
constrain(s, "Tangent", riser, END, first, START)
second = add(s, arc((60, 13), 5, 180, 270))
constrain(s, "Tangent", first, END, second, END)

touch = add(s, line((20, 25), (30, 25)))
constrain(s, "Tangent", touch, END, other)
constrain(s, "Horizontal", touch)

spoke = add(s, line((-15, -15), (-5, -5)))
constrain(s, "Perpendicular", spoke, big)

on_circle = add(s, point((10 * math.cos(math.radians(30)), 10 * math.sin(math.radians(30)))))
constrain(s, "PointOnObject", on_circle, START, big)

ring = add(s, circle((0, 0), 15))
constrain(s, "Coincident", ring, MID, big, MID)

sector = add(s, arc((90, 0), 10, 0, 60))
constrain(s, "Angle", sector, math.radians(60))
if at_least(1, 0):
    constrain(s, "Distance", sector, 10 * math.radians(60))

clockwise = Part.ArcOfCircle(Part.Circle(V(120, 0, 0), V(0, 0, -1), 10), 0.0, 1.0)
add(s, clockwise)
