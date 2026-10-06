# SPDX-License-Identifier: MIT
# A sketch of conics in a Body, on its YZ plane: an ellipse on the root
# point with its internal geometry (major and minor axis lines with
# lengths, the major one horizontal, the foci) and a line tangent to it; a
# turned ellipse without internal geometry, its centre placed by distances
# from the origin, with a point on it; an arc of an ellipse with its axes,
# a line joined tangent at its end; an arc of a hyperbola and of a
# parabola.

b = body("ConicsBody")
s = sketch_on(b, "Conics", plane="YZ_Plane")

ellipse = add(s, Part.Ellipse(V(20, 0, 0), V(0, 10, 0), V(0, 0, 0)))
constrain(s, "Coincident", ellipse, MID, H_AXIS, START)
s.exposeInternalGeometry(ellipse)
axes = {}
for c in s.Constraints:
    if c.Type == "InternalAlignment" and c.Second == ellipse:
        axes.setdefault(c.First, True)
lines = [i for i in sorted(axes) if s.Geometry[i].TypeId == "Part::GeomLineSegment"]
major = [i for i in lines if abs(s.Geometry[i].length() - 40) < 1e-6][0]
minor = [i for i in lines if abs(s.Geometry[i].length() - 20) < 1e-6][0]
constrain(s, "Distance", major, 40.0, name="major")
constrain(s, "Distance", minor, 20.0)
constrain(s, "Horizontal", major)
tangent = add(s, line((-10, 10), (10, 10)))
constrain(s, "Tangent", tangent, ellipse)

turn = math.radians(30)
u = (math.cos(turn), math.sin(turn))
turned = add(s, Part.Ellipse(V(50 + 15 * u[0], 15 * u[1], 0), V(50 - 8 * u[1], 8 * u[0], 0), V(50, 0, 0)))
constrain(s, "DistanceX", turned, MID, 50.0)
constrain(s, "DistanceY", turned, MID, 0.0)
on = add(s, point((50 - 8 * u[1], 8 * u[0])))
constrain(s, "PointOnObject", on, START, turned)

piece = add(s, Part.ArcOfEllipse(Part.Ellipse(V(20, 40, 0), V(0, 50, 0), V(0, 40, 0)), 0.0, math.pi / 2))
s.exposeInternalGeometry(piece)
lead = add(s, line((20, 20), (20, 40)))
constrain(s, "Tangent", lead, END, piece, START)

hyperbola = Part.Hyperbola()
hyperbola.MajorRadius = 10
hyperbola.MinorRadius = 5
hyperbola.Center = V(60, 40, 0)
add(s, Part.ArcOfHyperbola(hyperbola, -1.0, 1.0))

parabola = Part.Parabola()
parabola.Focal = 4
parabola.Center = V(90, 40, 0)
add(s, Part.ArcOfParabola(parabola, -8.0, 8.0))
