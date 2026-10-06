# SPDX-License-Identifier: MIT
# Expressions of integer counts, cone sizes and attachment turns (mitcad#4):
#
# - Spreadsheet: Count (5), ConeRadius (=6 mm), ConeHeight (=9 mm), Tilt
#   (=20 deg), Shift (=4 mm).
# - Rounded: a plate with a hole patterned Count / 2 times along X (FreeCAD
#   rounds 2.5 to 3; Mitcad's round()).
# - Cone: an additive cone, radii ConeRadius and 2, ConeHeight high, and a
#   Part cone, ConeHeight high, 270 degrees round.
# - Tilted: a sketch on the XY plane turned Tilt about its x axis, its
#   circle padded 5 mm.
# - Shifted: a sketch on the XY plane moved Shift along its x axis (a side
#   offset: Mitcad keeps FreeCAD's value), its rectangle padded 5 mm.
#
# The variant (expr_more_changed) has Count 7, ConeHeight 12 mm and Tilt
# 30 degrees.
#
# Mitcad expects: no fallback
# Mitcad change: Count = 7
# Mitcad change: ConeHeight = 12 mm
# Mitcad change: Tilt = 30 deg

sheet = spreadsheet("Spreadsheet", [
    ("B1", "5", "Count"),
    ("B2", "=6 mm", "ConeRadius"),
    ("B3", "=9 mm", "ConeHeight"),
    ("B4", "=20 deg", "Tilt"),
    ("B5", "=4 mm", "Shift"),
])

rounded = body("Rounded")
base = block(rounded, "RoundedBase", 60, 20, 5)
sketch = sketch_on_face(rounded, "RoundedHoleSketch", base, (0, 0, 1), 5)
add(sketch, circle((6, 10), 2))
hole = pocket(rounded, sketch, "RoundedHole", through_all=True)
pattern = rounded.newObject("PartDesign::LinearPattern", "RoundedHoles")
pattern.Originals = [hole]
pattern.Direction = (origin_feature(rounded, "X_Axis"), [""])
pattern.Length = 40
pattern.Occurrences = 3
bind(pattern, "Occurrences", "Spreadsheet.Count / 2")
doc.recompute()
rounded.Tip = pattern

coned = body("Cone")
cone = coned.newObject("PartDesign::AdditiveCone", "ConeAdd")
cone.MapMode = "Deactivated"
cone.Placement = place((0, 40, 0))
cone.Radius1, cone.Radius2, cone.Height = 6, 2, 9
bind(cone, "Radius1", "Spreadsheet.ConeRadius")
bind(cone, "Height", "Spreadsheet.ConeHeight")
part_cone = named(doc.addObject("Part::Cone", "PartCone"), "PartCone")
part_cone.Radius1, part_cone.Radius2, part_cone.Height, part_cone.Angle = 5, 0, 9, 270
part_cone.Placement = place((30, 40, 0))
bind(part_cone, "Height", "Spreadsheet.ConeHeight")

tilted = body("Tilted")
sketch = sketch_on(tilted, "TiltedSketch", offset=place((0, 0, 0), (1, 0, 0), 20))
add(sketch, circle((0, 80), 4))
bind(sketch, ".AttachmentOffset.Rotation.Angle", "Spreadsheet.Tilt")
pad(tilted, sketch, 5, "TiltedPad")

shifted = body("Shifted")
sketch = sketch_on(shifted, "ShiftedSketch", offset=place((4, 0, 0)))
rectangle(sketch, 40, 70, 10, 6)
bind(sketch, ".AttachmentOffset.Base.x", "Spreadsheet.Shift")
pad(shifted, sketch, 5, "ShiftedPad")
doc.recompute()


def changed():
    sheet.set("B1", "7")
    sheet.set("B3", "=12 mm")
    sheet.set("B4", "=30 deg")


variant(changed)
