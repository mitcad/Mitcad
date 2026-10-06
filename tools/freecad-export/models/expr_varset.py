# SPDX-License-Identifier: MIT
# Bodies driven by a VarSet (FreeCAD 1.0 and later; skipped in 0.21):
#
# - Params: Side (a length, 30 mm), Turn (an angle, 270 degrees), Copies
#   (an integer, 4), Ratio (a float, 0.25), and bound to expressions of
#   them Thick (=Side * Ratio), Bore (=Thick / 3), Spot (=Side / 2).
# - Disk: a circle of radius Side padded Thick; a hole of radius Bore,
#   Spot from the axis, on a sketch Thick above the XY plane, pocketed
#   through; Copies of them round the Z axis.
# - Ring: a rectangle Thick wide and Side high, Side + 10 mm from the Z
#   axis on the XZ plane, turned Turn about the sketch's V axis.
#
# The variant (expr_varset_changed) has Side 40 mm.
#
# Mitcad expects: no fallback
# Mitcad change: Side = 40 mm

params = varset(
    "Params",
    [
        ("Length", "Side", 30),
        ("Angle", "Turn", 270),
        ("Integer", "Copies", 4),
        ("Float", "Ratio", 0.25),
        ("Length", "Thick", "=Side * Ratio"),
        ("Length", "Bore", "=Thick / 3"),
        ("Length", "Spot", "=Side / 2"),
    ],
)

disk_body = body("Disk")
sketch = sketch_on(disk_body, "DiskSketch")
rim = add(sketch, circle((0, 0), 30))
constrain(sketch, "Coincident", rim, MID, H_AXIS, START)
size = constrain(sketch, "Radius", rim, 30)
bind(sketch, "Constraints[%d]" % size, "Params.Side")
disk = pad(disk_body, sketch, 7.5, "DiskPad")
bind(disk, "Length", "Params.Thick")
doc.recompute()

holes = sketch_on(disk_body, "HoleSketch", offset=place((0, 0, 7.5)))
bind(holes, ".AttachmentOffset.Base.z", "Params.Thick")
hole = add(holes, circle((15, 0), 2.5))
constrain(holes, "PointOnObject", hole, MID, H_AXIS)
spot = constrain(holes, "DistanceX", H_AXIS, START, hole, MID, 15)
bind(holes, "Constraints[%d]" % spot, "Params.Spot")
bore = constrain(holes, "Radius", hole, 2.5)
bind(holes, "Constraints[%d]" % bore, "Params.Bore")
hole_pocket = pocket(disk_body, holes, "HolePocket", through_all=True)
doc.recompute()

pattern = disk_body.newObject("PartDesign::PolarPattern", "HoleRing")
pattern.Originals = [hole_pocket]
pattern.Axis = (origin_feature(disk_body, "Z_Axis"), [""])
pattern.Angle = 360
pattern.Occurrences = 4
bind(pattern, "Occurrences", "Params.Copies")
doc.recompute()
disk_body.Tip = pattern

ring_body = body("Ring")
profile = sketch_on(ring_body, "RingSketch", plane="XZ_Plane")
lines = rectangle(profile, 40, 0, 7.5, 30)
constrain(profile, "PointOnObject", lines[0], START, H_AXIS)
constrain(profile, "Horizontal", lines[0])
constrain(profile, "Horizontal", lines[2])
constrain(profile, "Vertical", lines[1])
constrain(profile, "Vertical", lines[3])
inner = constrain(profile, "DistanceX", H_AXIS, START, lines[0], START, 40)
bind(profile, "Constraints[%d]" % inner, "Params.Side + 10 mm")
width = constrain(profile, "DistanceX", lines[0], START, lines[0], END, 7.5)
bind(profile, "Constraints[%d]" % width, "Params.Thick")
height = constrain(profile, "DistanceY", lines[1], START, lines[1], END, 30)
bind(profile, "Constraints[%d]" % height, "Params.Side")
revolution = ring_body.newObject("PartDesign::Revolution", "RingTurn")
revolution.Profile = profile
revolution.ReferenceAxis = (profile, ["V_Axis"])
revolution.Angle = 270
bind(revolution, "Angle", "Params.Turn")
profile.Visibility = False
doc.recompute()


def larger():
    params.Side = 40


variant(larger)
