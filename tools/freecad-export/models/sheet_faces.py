# SPDX-License-Identifier: MIT
# Shapes without solids: a Part::Plane (one face) placed and turned, a
# Part::Extrusion of an open polyline into a sheet (Solid false), and a
# Part::Face of a closed sketch; a Part::Line (an edge only) is no body.

plane = named(doc.addObject("Part::Plane", "Plane"), "Plane")
plane.Length, plane.Width = 30, 20
plane.Placement = place((0, 0, 10), (1, 0, 0), 20)

profile = doc.addObject("Sketcher::SketchObject", "OpenProfile")
profile.Label = "OpenProfile"
points = [(0, 0), (10, 0), (15, 8), (25, 8)]
for i in range(len(points) - 1):
    a, b = points[i], points[i + 1]
    profile.addGeometry(Part.LineSegment(V(a[0], a[1], 0), V(b[0], b[1], 0)), False)
for i in range(len(points) - 2):
    profile.addConstraint(Sketcher.Constraint("Coincident", i, 2, i + 1, 1))
profile.Placement = place((40, 0, 0))
wall = named(doc.addObject("Part::Extrusion", "Wall"), "Wall")
wall.Base = profile
wall.Dir = V(0, 0, 15)
wall.Solid = False
hide(profile)

loop = doc.addObject("Sketcher::SketchObject", "ClosedProfile")
loop.Label = "ClosedProfile"
square = [(0, 0), (12, 0), (12, 12), (0, 12)]
for i in range(4):
    a, b = square[i], square[(i + 1) % 4]
    loop.addGeometry(Part.LineSegment(V(a[0], a[1], 0), V(b[0], b[1], 0)), False)
for i in range(4):
    loop.addConstraint(Sketcher.Constraint("Coincident", i, 2, (i + 1) % 4, 1))
loop.Placement = place((0, 40, 0), (0, 1, 0), 45)
face = named(doc.addObject("Part::Face", "Face"), "Face")
face.Sources = [loop]
hide(loop)

line = named(doc.addObject("Part::Line", "Line"), "Line")
line.X2, line.Y2, line.Z2 = 10, 10, 10
