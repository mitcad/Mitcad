# SPDX-License-Identifier: MIT
# Part workbench objects whose sizes a spreadsheet drives:
#
# - Dims: PlateL (=40 mm), PlateW (=PlateL / 2), PlateH (=PlateL / 4),
#   PegR (=PlateH / 2) (FreeCAD takes no unit names, such as L or H, as
#   aliases).
# - Plate: a box PlateL x PlateW x PlateH with a cylinder of radius PegR
#   and height 2 PlateH at (10, 10) cut out of it (Part::Cut; the operands
#   hidden).
# - Post: a circle sketch of radius PegR (an expression of the sketch's
#   radius) 60 mm off, extruded 3 PlateH (Part::Extrusion).
#
# The variant (expr_part_changed) has PlateL 60 mm.
#
# Mitcad expects: no fallback
# Mitcad change: PlateL = 60 mm

dims = spreadsheet(
    "Dims",
    [
        ("B1", "=40 mm", "PlateL"),
        ("B2", "=PlateL / 2", "PlateW"),
        ("B3", "=PlateL / 4", "PlateH"),
        ("B4", "=PlateH / 2", "PegR"),
    ],
)

plate = named(doc.addObject("Part::Box", "PlateBox"), "PlateBox")
plate.Length, plate.Width, plate.Height = 40, 20, 10
bind(plate, "Length", "Dims.PlateL")
bind(plate, "Width", "Dims.PlateW")
bind(plate, "Height", "Dims.PlateH")
peg = named(doc.addObject("Part::Cylinder", "Peg"), "Peg")
peg.Radius, peg.Height = 5, 20
peg.Placement = place((10, 10, -5))
bind(peg, "Radius", "Dims.PegR")
bind(peg, "Height", "Dims.PlateH * 2")
cut = named(doc.addObject("Part::Cut", "Plate"), "Plate")
cut.Base = plate
cut.Tool = peg
hide(plate)
hide(peg)

post_sketch = doc.addObject("Sketcher::SketchObject", "PostSketch")
post_sketch.Label = "PostSketch"
rim = add(post_sketch, circle((60, 0), 5))
constrain(post_sketch, "PointOnObject", rim, MID, H_AXIS)
constrain(post_sketch, "DistanceX", H_AXIS, START, rim, MID, 60)
size = constrain(post_sketch, "Radius", rim, 5)
bind(post_sketch, "Constraints[%d]" % size, "Dims.PegR")
post = named(doc.addObject("Part::Extrusion", "Post"), "Post")
post.Base = post_sketch
post.DirMode = "Normal"
post.Solid = True
post.LengthFwd = 30
bind(post, "LengthFwd", "Dims.PlateH * 3")
hide(post_sketch)
doc.recompute()


def larger():
    dims.set("B1", "=60 mm")


variant(larger)
