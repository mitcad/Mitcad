# SPDX-License-Identifier: MIT
# Revolutions and grooves, one option per Body: a 5 x 10 rectangle on the
# XZ plane 10 mm from the sketch's V axis (moved along X by the sketch's
# attachment offset), turned about that axis:
#
# - Full: 360 degrees, a ring.
# - Quarter: 90 degrees.
# - Symmetric: 90 degrees in all about the sketch plane (Midplane).
# - Reversed: 90 degrees the other way.
# - Construction: about a construction line of the sketch (Axis0).
# - Edge: about an edge of the profile itself (its left side, Edge4).
# - Origin: about the Body's Z axis (the sketch not moved).
# - Groove: a disk (a padded circle) with a ring groove turned out of it
#   about the V axis.
#
# Mitcad expects: no fallback


def ring_sketch(container, name, x):
    sketch = sketch_on(container, name, plane="XZ_Plane", offset=place((x, 0, 0)))
    rectangle(sketch, 10, 0, 5, 10)
    return sketch


def revolution(container, sketch, name, angle=360, axis="V_Axis"):
    feature = container.newObject("PartDesign::Revolution", name)
    feature.Profile = sketch
    feature.ReferenceAxis = (sketch, [axis])
    feature.Angle = angle
    sketch.Visibility = False
    return feature


full = body("Full")
revolution(full, ring_sketch(full, "FullSketch", 0), "FullRevolution")

quarter = body("Quarter")
revolution(quarter, ring_sketch(quarter, "QuarterSketch", 40), "QuarterRevolution", 90)

symmetric = body("Symmetric")
feature = revolution(symmetric, ring_sketch(symmetric, "SymmetricSketch", 80), "SymmetricRevolution", 90)
feature.Midplane = True

reversed_body = body("Reversed")
feature = revolution(reversed_body, ring_sketch(reversed_body, "ReversedSketch", 120), "ReversedRevolution", 90)
feature.Reversed = True

construction = body("Construction")
sketch = ring_sketch(construction, "ConstructionSketch", 160)
add(sketch, line((-5, -5), (-5, 15)), construction=True)
revolution(construction, sketch, "ConstructionRevolution", 120, axis="Axis0")

edge = body("Edge")
sketch = sketch_on(edge, "EdgeSketch", plane="XZ_Plane", offset=place((200, 0, 0)))
rectangle(sketch, 20, 0, 10, 10)
revolution(edge, sketch, "EdgeRevolution", 180, axis="Edge4")

origin = body("Origin")
sketch = sketch_on(origin, "OriginSketch", plane="XZ_Plane")
rectangle(sketch, 20, 0, 4, 6)
feature = origin.newObject("PartDesign::Revolution", "OriginRevolution")
feature.Profile = sketch
feature.ReferenceAxis = (origin_feature(origin, "Z_Axis"), [""])
feature.Angle = 270
sketch.Visibility = False

groove_body = body("Grooved")
disk = sketch_on(groove_body, "DiskSketch", offset=place((0, -60, 0)))
add(disk, circle((0, 0), 15))
pad(groove_body, disk, 20, "DiskPad")
# The XZ plane's normal is -Y: 60 along it is y = -60.
sketch = sketch_on(groove_body, "GrooveSketch", plane="XZ_Plane", offset=place((0, 0, 60)))
rectangle(sketch, 12, 8, 4, 4)
groove = groove_body.newObject("PartDesign::Groove", "Groove")
groove.Profile = sketch
groove.ReferenceAxis = (sketch, ["V_Axis"])
groove.Angle = 360
sketch.Visibility = False
