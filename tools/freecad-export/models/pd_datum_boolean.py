# SPDX-License-Identifier: MIT
# Datums, PartDesign booleans, the Body's Tip and suppressed features:
#
# - Datums: a datum plane 15 mm above the XY plane (attached, offset),
#   with a circle padded down from it to the XY plane; a datum point.
# - AboutDatum: a datum line (placed, not attached) that a rectangle is
#   turned about.
# - Target: a block with a cylinder of the Tool Body cut out of it
#   (PartDesign::Boolean, Cut); Tool hidden.
# - Fused: a block with the Extra Body's block fused to it (Fuse).
# - Tipped: a pad, a pocket and a second pad, the Tip at the pocket (the
#   second pad rolled back).
# - Suppressed (FreeCAD 1.0 and later): a pad, a suppressed pocket and a
#   second pad.
#
# Mitcad expects: no fallback

datums = body("Datums")
plane = datums.newObject("PartDesign::Plane", "DatumPlane")
attach(plane, [(origin_feature(datums, "XY_Plane"), "")], "FlatFace")
plane.AttachmentOffset = place((0, 0, 15))
doc.recompute()
sketch = datums.newObject("Sketcher::SketchObject", "OnDatumSketch")
sketch.Label = "OnDatumSketch"
attach(sketch, [(plane, "")])
add(sketch, circle((10, 10), 6))
pad(datums, sketch, 15, "OnDatumPad", reversed_=True)
# Its own Body: two solids in one Body differ between versions.
about = body("AboutDatum")
axis = about.newObject("PartDesign::Line", "DatumAxis")
axis.MapMode = "Deactivated"
axis.Placement = place((40, 0, 0))
profile = sketch_on(about, "AboutAxisSketch", plane="XZ_Plane")
rectangle(profile, 45, 0, 5, 8)
revolution = about.newObject("PartDesign::Revolution", "AboutAxis")
revolution.Profile = profile
revolution.ReferenceAxis = (axis, [""])
revolution.Angle = 180
profile.Visibility = False
point = datums.newObject("PartDesign::Point", "DatumPoint")
point.MapMode = "Deactivated"
point.Placement = place((0, 0, 30))


def add_bodies(feature, bodies):
    if hasattr(feature, "setObjects"):
        feature.setObjects(bodies)
    else:
        feature.addObjects(bodies)


tool = body("Tool")
sketch = sketch_on(tool, "ToolSketch")
add(sketch, circle((15, -40), 6))
pad(tool, sketch, 20, "ToolPad")
target = body("Target")
block(target, "TargetPad", 30, 20, 10, 0, -50)
cut = target.newObject("PartDesign::Boolean", "TargetCut")
add_bodies(cut, [tool])
cut.Type = "Cut"
hide(tool)

extra = body("Extra")
block(extra, "ExtraPad", 10, 10, 20, 60, -45)
fused = body("Fused")
block(fused, "FusedPad", 30, 20, 10, 50, -50)
fuse = fused.newObject("PartDesign::Boolean", "FusedBoolean")
add_bodies(fuse, [extra])
fuse.Type = "Fuse"
hide(extra)

tipped = body("Tipped")
first = block(tipped, "TippedPad", 30, 20, 10, 0, -100)
sketch = sketch_on_face(tipped, "TippedHoleSketch", first, (0, 0, 1), 10)
add(sketch, circle((15, -90), 4))
held = pocket(tipped, sketch, "TippedPocket", length=5)
sketch = sketch_on_face(tipped, "TippedBossSketch", first, (0, 0, 1), 10)
add(sketch, circle((5, -95), 3))
pad(tipped, sketch, 4, "TippedBoss")
doc.recompute()
tipped.Tip = held

if at_least(1, 0):
    suppressed = body("Suppressed")
    first = block(suppressed, "SuppressedPad", 30, 20, 10, 50, -100)
    sketch = sketch_on_face(suppressed, "SuppressedHoleSketch", first, (0, 0, 1), 10)
    add(sketch, circle((65, -90), 4))
    off = pocket(suppressed, sketch, "SuppressedPocket", length=5)
    sketch = sketch_on_face(suppressed, "SuppressedBossSketch", first, (0, 0, 1), 10)
    add(sketch, circle((55, -95), 3))
    pad(suppressed, sketch, 4, "SuppressedBoss")
    off.Suppressed = True
