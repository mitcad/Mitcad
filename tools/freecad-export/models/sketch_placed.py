# SPDX-License-Identifier: MIT
# Sketches in placed containers: an App::Part moved and turned holding a
# Body with a placement of its own, whose sketch lies on the Body's XZ plane
# turned and moved by its attachment offset (a slot padded into the Body);
# a sketch in the part itself on the part's YZ plane; an open polyline at
# the document's top, not attached, moved along Z by its placement.

holder = part("Frame", place((20, 10, 0), (0, 0, 1), 30))
inner = body("Inner", place((0, 0, 15), (1, 0, 0), 20), parent=holder)
slot = sketch_on(inner, "Slot", plane="XZ_Plane", offset=place((5, 2, 3), (0, 0, 1), 15))
ends = [(0, 0), (20, 0), (20, 8), (0, 8)]
edges = polyline(slot, ends, closed=True)
constrain(slot, "Horizontal", edges[0])
constrain(slot, "Vertical", edges[1])
constrain(slot, "Horizontal", edges[2])
constrain(slot, "Vertical", edges[3])
constrain(slot, "Coincident", edges[0], START, H_AXIS, START)
constrain(slot, "DistanceX", edges[0], 20.0)
constrain(slot, "DistanceY", edges[1], 8.0)
pad(inner, slot, 6, "SlotPad")

side = sketch_on(holder, "Side", plane="YZ_Plane")
add(side, circle((10, 10), 5))
constrain(side, "Radius", 0, 5.0)

loose = doc.addObject("Sketcher::SketchObject", "Loose")
loose.Label = "Loose"
loose.Placement = place((0, 0, -12))
polyline(loose, [(0, 0), (10, 0), (10, 10)])
