# SPDX-License-Identifier: MIT
# Sketches of lines and points in a Body, on its XY plane: every
# constraint between lines and points.
#
# Outline: a fully constrained pentagon from the origin (coincident with
# the root point, horizontal, vertical, a named length, horizontal and
# vertical distances of points and of one point from the origin, a point on
# the V axis, the angle between two lines, a reference angle of one line)
# with a construction diagonal, and a length bound by an expression to the
# named one.
#
# Relations (offset 5 mm up): parallel, equal, perpendicular lines (edge to
# edge and end to end), a point's distance from a line, horizontal and
# vertical points, points symmetric about a line, about a point and about
# the H axis, a point on the H axis, horizontal distances of a line and a
# negative one, a point-to-point distance, a blocked line, a switched-off
# constraint and a reference distance.

b = body("LinesBody")

outline = sketch_on(b, "Outline")
P = [(0, 0), (40, 0), (40, 20), (25, 30), (0, 20)]
L = polyline(outline, P, closed=True)
constrain(outline, "Coincident", L[0], START, H_AXIS, START)
constrain(outline, "Horizontal", L[0])
constrain(outline, "Distance", L[0], 40.0, name="width")
constrain(outline, "Vertical", L[1])
constrain(outline, "DistanceY", L[1], START, L[1], END, 20.0, name="height")
constrain(outline, "DistanceX", L[2], END, 25.0)
constrain(outline, "DistanceY", L[2], END, 30.0)
constrain(outline, "PointOnObject", L[3], END, V_AXIS)
constrain(outline, "Angle", L[3], L[4], direction_angle(P[4], P[0]) - direction_angle(P[3], P[4]))
constrain(outline, "Angle", L[2], direction_angle(P[2], P[3]), driving=False)
diagonal = add(outline, line(P[0], P[2]), True)
constrain(outline, "Coincident", diagonal, START, L[0], START)
constrain(outline, "Coincident", diagonal, END, L[1], END)
outline.setExpression(".Constraints.height", ".Constraints.width / 2")

relations = sketch_on(b, "Relations", offset=place((0, 0, 5)))
a = add(relations, line((0, 40), (30, 40)))
c = add(relations, line((0, 50), (30, 50)))
constrain(relations, "Horizontal", a)
constrain(relations, "Parallel", a, c)
constrain(relations, "Equal", a, c)
constrain(relations, "Distance", c, START, a, 10.0)
constrain(relations, "DistanceX", a, START, c, END, 30.0)
upright = add(relations, line((40, 40), (40, 60)))
constrain(relations, "Perpendicular", upright, a)
arm = add(relations, line((40, 60), (55, 60)))
constrain(relations, "Perpendicular", upright, END, arm, START)
constrain(relations, "DistanceX", arm, 15.0)
constrain(relations, "DistanceY", upright, 20.0, driving=False)
g = add(relations, point((60, 40)))
h = add(relations, point((60, 70)))
constrain(relations, "Horizontal", g, START, upright, START)
constrain(relations, "DistanceX", g, START, upright, START, -20.0)
constrain(relations, "Vertical", g, START, h, START)
constrain(relations, "Distance", g, START, h, START, 30.0)
s1 = add(relations, point((45, 45)))
s2 = add(relations, point((45, 35)))
constrain(relations, "Symmetric", s1, START, s2, START, a)
m1 = add(relations, point((70, 40)))
m2 = add(relations, point((80, 50)))
middle = add(relations, point((75, 45)))
constrain(relations, "Symmetric", m1, START, m2, START, middle, START)
x1 = add(relations, point((5, -5)))
x2 = add(relations, point((5, 5)))
constrain(relations, "Symmetric", x1, START, x2, START, H_AXIS)
on_axis = add(relations, point((20, 0)))
constrain(relations, "PointOnObject", on_axis, START, H_AXIS)
blocked = add(relations, line((0, 70), (10, 75)))
constrain(relations, "Block", blocked)
constrain(relations, "Horizontal", blocked, active=False)
