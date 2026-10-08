# SPDX-License-Identifier: MIT
# Attachment offsets that turn datum planes and sketches about other axes
# than their support's x and y axes, bound to expressions:
#
# - Spreadsheet: Turn (=30 deg), Spin (=20 deg), Lean (=15 deg).
# - DiagonalPlane: a datum plane on the XY plane turned Turn about the
#   line x = y in it, a circle on it padded 5 mm (Mitcad: a construction
#   plane at an angle about that line, following Turn).
# - DiagonalSketch: a sketch on the XZ plane turned Turn about the line
#   x = -y of its support, its rectangle padded 4 mm (likewise).
# - SpunSketch: a sketch on the XY plane turned Spin about its normal, its
#   rectangle padded 3 mm. Mitcad's planes turn only about lines in them,
#   so the sketch lies on a plane fixed at FreeCAD's placement and Spin
#   keeps its value.
# - LeaningPlane: a datum plane on the XY plane turned Lean about the axis
#   (1, 0, 1), which leaves its support's plane: fixed likewise.
#
# The variant (expr_datum_turns_changed) has Turn 40 degrees (Spin and
# Lean unchanged).
#
# Mitcad expects: no fallback
# Mitcad change: Turn = 40 deg

sheet = spreadsheet("Spreadsheet", [
    ("B1", "=30 deg", "Turn"),
    ("B2", "=20 deg", "Spin"),
    ("B3", "=15 deg", "Lean"),
])

diagonal = body("DiagonalPlane")
plane = diagonal.newObject("PartDesign::Plane", "DiagonalDatum")
attach(plane, [(origin_feature(diagonal, "XY_Plane"), "")], "FlatFace")
plane.AttachmentOffset = place((0, 0, 0), (1, 1, 0), 30)
bind(plane, ".AttachmentOffset.Rotation.Angle", "Spreadsheet.Turn")
hide(plane)
doc.recompute()
sketch = diagonal.newObject("Sketcher::SketchObject", "DiagonalDatumSketch")
sketch.Label = "DiagonalDatumSketch"
attach(sketch, [(plane, "")])
add(sketch, circle((10, -10), 4))
pad(diagonal, sketch, 5, "DiagonalPad")

turned = body("DiagonalSketch")
sketch = sketch_on(turned, "DiagonalSketchSketch", plane="XZ_Plane",
                   offset=place((0, 0, 0), (1, -1, 0), 30))
bind(sketch, ".AttachmentOffset.Rotation.Angle", "Spreadsheet.Turn")
rectangle(sketch, 30, 2, 8, 5)
pad(turned, sketch, 4, "DiagonalSketchPad")

spun = body("SpunSketch")
sketch = sketch_on(spun, "SpunSketchSketch", offset=place((0, 0, 0), (0, 0, 1), 20))
bind(sketch, ".AttachmentOffset.Rotation.Angle", "Spreadsheet.Spin")
rectangle(sketch, 40, 40, 12, 6)
pad(spun, sketch, 3, "SpunPad")

leaning = body("LeaningPlane")
plane = leaning.newObject("PartDesign::Plane", "LeaningDatum")
attach(plane, [(origin_feature(leaning, "XY_Plane"), "")], "FlatFace")
plane.AttachmentOffset = place((0, 0, 0), (1, 0, 1), 15)
bind(plane, ".AttachmentOffset.Rotation.Angle", "Spreadsheet.Lean")
hide(plane)
doc.recompute()
sketch = leaning.newObject("Sketcher::SketchObject", "LeaningSketch")
sketch.Label = "LeaningSketch"
attach(sketch, [(plane, "")])
add(sketch, circle((-30, 10), 3))
pad(leaning, sketch, 4, "LeaningPad")
doc.recompute()


def changed():
    sheet.set("B1", "=40 deg")


variant(changed)
