# SPDX-License-Identifier: MIT
# Attachment offsets that turn datum planes and sketches, bound to
# expressions (Mitcad: construction planes at an angle or offset by them):
#
# - Spreadsheet: Tilt (=25 deg), Shift (=12 mm), Side (=8 mm).
# - TurnedPlane: a datum plane on the XY plane turned Tilt about its x
#   axis, a circle on it padded 5 mm.
# - QuarterPlane: a datum plane on the XY plane turned a quarter about its
#   y axis and moved Shift along its x axis (so the plane is Shift from the
#   YZ plane), a rectangle on it padded 5 mm.
# - QuarterSketch: a sketch on the XY plane turned a quarter back about
#   its x axis and moved Side along its y axis (Side from the XZ plane),
#   its rectangle padded 4 mm.
#
# The variant (expr_datum_offsets_changed) has Tilt 35 degrees, Shift 15 mm
# and Side 10 mm.
#
# Mitcad expects: no fallback
# Mitcad change: Tilt = 35 deg
# Mitcad change: Shift = 15 mm
# Mitcad change: Side = 10 mm

sheet = spreadsheet("Spreadsheet", [
    ("B1", "=25 deg", "Tilt"),
    ("B2", "=12 mm", "Shift"),
    ("B3", "=8 mm", "Side"),
])

turned = body("TurnedPlane")
plane = turned.newObject("PartDesign::Plane", "TiltPlane")
attach(plane, [(origin_feature(turned, "XY_Plane"), "")], "FlatFace")
plane.AttachmentOffset = place((0, 0, 0), (1, 0, 0), 25)
bind(plane, ".AttachmentOffset.Rotation.Angle", "Spreadsheet.Tilt")
hide(plane)
doc.recompute()
sketch = turned.newObject("Sketcher::SketchObject", "TiltSketch")
sketch.Label = "TiltSketch"
attach(sketch, [(plane, "")])
add(sketch, circle((0, 20), 4))
pad(turned, sketch, 5, "TiltPad")

quarter = body("QuarterPlane")
plane = quarter.newObject("PartDesign::Plane", "ShiftPlane")
attach(plane, [(origin_feature(quarter, "XY_Plane"), "")], "FlatFace")
plane.AttachmentOffset = place((12, 0, 0), (0, 1, 0), 90)
bind(plane, ".AttachmentOffset.Base.x", "Spreadsheet.Shift")
hide(plane)
doc.recompute()
sketch = quarter.newObject("Sketcher::SketchObject", "ShiftSketch")
sketch.Label = "ShiftSketch"
attach(sketch, [(plane, "")])
rectangle(sketch, 2, 40, 6, 4)
pad(quarter, sketch, 5, "ShiftPad")

side = body("QuarterSketch")
sketch = sketch_on(side, "SideSketch", offset=place((0, 8, 0), (1, 0, 0), -90))
bind(sketch, ".AttachmentOffset.Base.y", "Spreadsheet.Side")
rectangle(sketch, 30, 2, 6, 5)
pad(side, sketch, 4, "SidePad")
doc.recompute()


def changed():
    sheet.set("B1", "=35 deg")
    sheet.set("B2", "=15 mm")
    sheet.set("B3", "=10 mm")


variant(changed)
