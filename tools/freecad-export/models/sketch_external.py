# SPDX-License-Identifier: MIT
# Sketches with external geometry (edges and a vertex of other objects
# projected into the sketch), and sketches on faces:
#
# - OnTop: on the top face of a padded block (the Body's tip), with two
#   edges of that face and a vertex of the block as external geometry; a
#   line from the first edge's start along it, a circle centred on the
#   vertex, a point on the second edge; from FreeCAD 1.1 also a defining
#   external edge (part of the sketch's profile).
# - Below: on the XY plane of a Body whose tip is a pocket, with an edge
#   of the pad before it (not of the Body's final shape).
# - Beside: at the document's top, on a face of a Part box, with one of its
#   edges.


def local(sketch, at):
    """A model point in the sketch's coordinates (x, y)."""
    p = sketch.Placement.inverse().multVec(V(*at))
    return (p.x, p.y)


block = body("Block")
block_pad = pad(block, rectangle_sketch(block, 40, 30, "BlockSketch"), 10, "BlockPad")
doc.recompute()

top = block.newObject("Sketcher::SketchObject", "OnTop")
top.Label = "OnTop"
attach(top, [(block_pad, face_index(block_pad.Shape, (0, 0, 1), 10))])
doc.recompute()
shape = block_pad.Shape
top.addExternal(block_pad.Name, edge_index(shape, (0, 0, 10), (40, 0, 10)))
top.addExternal(block_pad.Name, edge_index(shape, (40, 0, 10), (40, 30, 10)))
top.addExternal(block_pad.Name, vertex_index(shape, (0, 30, 10)))
first_edge, second_edge, corner = -3, -4, -5
a, b = local(top, (0, 0, 10)), local(top, (20, 0, 10))
along = add(top, line(a, b))
constrain(top, "Coincident", along, START, first_edge, START)
constrain(top, "PointOnObject", along, END, first_edge)
ring = add(top, circle(local(top, (0, 30, 10)), 4))
constrain(top, "Coincident", ring, MID, corner, START)
constrain(top, "Radius", ring, 4.0)
dot = add(top, point(local(top, (40, 15, 10))))
constrain(top, "PointOnObject", dot, START, second_edge)
if at_least(1, 1):
    top.addExternal(block_pad.Name, edge_index(shape, (0, 0, 10), (0, 30, 10)), True, False)

notched = body("Notched")
notched_pad = pad(notched, rectangle_sketch(notched, 30, 30, "NotchedSketch", 60, 0), 10, "NotchedPad")
pocket(notched, circle_sketch(notched, 5, "NotchSketch", 75, 15), "NotchPocket", through_all=True)
doc.recompute()
below = sketch_on(notched, "Below")
below.addExternal(notched_pad.Name, edge_index(notched_pad.Shape, (60, 0, 0), (90, 0, 0)))
under = add(below, line((60, 0), (75, 0)))
constrain(below, "Coincident", under, START, -3, START)
constrain(below, "PointOnObject", under, END, -3)

crate = named(doc.addObject("Part::Box", "Crate"), "Crate")
crate.Length, crate.Width, crate.Height = 20, 20, 20
crate.Placement = place((0, -60, 0))
doc.recompute()
beside = doc.addObject("Sketcher::SketchObject", "Beside")
beside.Label = "Beside"
attach(beside, [(crate, face_index(crate.Shape, (0, -1, 0)))])
doc.recompute()
beside.addExternal(crate.Name, edge_index(crate.Shape, (0, -60, 0), (20, -60, 0)))
c = add(beside, circle(local(beside, (10, -60, 10)), 3))
constrain(beside, "Radius", c, 3.0)
