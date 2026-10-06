# SPDX-License-Identifier: MIT
# A sketch with the rarer constraints, in an App::Part (moved and turned),
# not attached to anything, its own placement turned: a blocked circle with
# a construction circle around it, two lines at an angle through a point
# (angle via point), a ray refracted at a line (Snell's law), obtuse angles
# between lines (from one point, and at a corner from its ends), and from
# FreeCAD 1.0 the distances of a point from a circle and between two
# circles.

holder = part("Holder", place((100, 0, 0), (0, 0, 1), 90))
s = holder.newObject("Sketcher::SketchObject", "Special")
s.Label = "Special"
s.Placement = place((5, 5, 5), (1, 0, 0), 90)

fixed = add(s, circle((0, 0), 6))
constrain(s, "Block", fixed)
guide = add(s, circle((0, 0), 9), True)
constrain(s, "Coincident", guide, MID, fixed, MID)
constrain(s, "Radius", guide, 9.0)

corner = (20.0, 0.0)
first = add(s, line((10, -10), (30, 10)))
second = add(s, line((10, 10), (30, -10)))
pivot = add(s, point(corner))
constrain(s, "PointOnObject", pivot, START, first)
constrain(s, "PointOnObject", pivot, START, second)
constrain(s, "AngleViaPoint", first, second, pivot, START, math.radians(-90))

boundary = add(s, line((40, 0), (80, 0)))
ray_in = add(s, line((45, 10), (60, 0)))
ray_out = add(s, line((60, 0), (75, -10)))
constrain(s, "Horizontal", boundary)
try:
    constrain(s, "SnellsLaw", ray_in, END, ray_out, START, boundary, 1.0)
except Exception:
    constrain(s, "Coincident", ray_in, END, ray_out, START)

# Obtuse angles: between two lines from one point (from the first's
# direction to the second's), and between two lines at a corner measured
# from the corner's ends.
base, wide = (0.0, -30.0), math.radians(135)
first_arm = add(s, line(base, (10.0, -30.0)))
second_arm = add(s, line(base, (base[0] + 10 * math.cos(wide), base[1] + 10 * math.sin(wide))))
constrain(s, "Coincident", first_arm, START, second_arm, START)
constrain(s, "Angle", first_arm, second_arm, wide)
corner, turn = (40.0, -30.0), math.radians(60)
leg = add(s, line((30.0, -30.0), corner))
other_leg = add(s, line(corner, (corner[0] + 10 * math.cos(turn), corner[1] + 10 * math.sin(turn))))
constrain(s, "Coincident", leg, END, other_leg, START)
# From the corner back along the first leg (-x), to along the second.
between = math.atan2(-math.sin(turn), -math.cos(turn))
constrain(s, "Angle", leg, END, other_leg, START, between)

if at_least(1, 0):
    dot = add(s, point((0, 20)))
    constrain(s, "Distance", dot, START, fixed, 14.0)
    ring = add(s, circle((30, 30), 4))
    constrain(s, "Distance", fixed, ring, math.hypot(30, 30) - 10)
