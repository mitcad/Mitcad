# SPDX-License-Identifier: MIT
# MultiTransform features Mitcad replays as patterns and mirrors of the
# originals, one per Body:
#
# - TwoLinear: a hole pocketed through a plate, 3 along the X axis over 40
#   mm, then 2 along the Y axis over 10 mm (a rectangular pattern).
# - TwoMirrors: a boss on a plate mirrored about the YZ plane, then about
#   the XZ plane (four bosses: two mirrors and a half turn).
# - OnePolar: a hole through a disk, 5 round its axis (one transformation).
#
# Mitcad expects: no fallback


def multi(container, name, originals, steps):
    feature = container.newObject("PartDesign::MultiTransform", name)
    feature.Originals = originals
    made = []
    for kind, options in steps:
        step = container.newObject("PartDesign::" + kind, name + kind + str(len(made)))
        for key, value in options.items():
            setattr(step, key, value)
        made.append(step)
    feature.Transformations = made
    return feature


plate = body("TwoLinear")
base = block(plate, "TwoLinearBase", 60, 30, 5, 0, 0)
sketch = sketch_on_face(plate, "TwoLinearHoleSketch", base, (0, 0, 1), 5)
add(sketch, circle((8, 8), 2))
hole = pocket(plate, sketch, "TwoLinearHole", through_all=True)
multi(plate, "TwoLinearHoles", [hole], [
    ("LinearPattern", {"Direction": (origin_feature(plate, "X_Axis"), [""]), "Length": 40, "Occurrences": 3}),
    ("LinearPattern", {"Direction": (origin_feature(plate, "Y_Axis"), [""]), "Length": 10, "Occurrences": 2}),
])

mirrored = body("TwoMirrors")
base = block(mirrored, "TwoMirrorsBase", 40, 30, 5, -20, -15)
sketch = sketch_on_face(mirrored, "TwoMirrorsBossSketch", base, (0, 0, 1), 5)
add(sketch, circle((10, 6), 3))
boss = pad(mirrored, sketch, 4, "TwoMirrorsBoss")
multi(mirrored, "TwoMirrorsBosses", [boss], [
    ("Mirrored", {"MirrorPlane": (origin_feature(mirrored, "YZ_Plane"), [""])}),
    ("Mirrored", {"MirrorPlane": (origin_feature(mirrored, "XZ_Plane"), [""])}),
])

polar = body("OnePolar")
disk = sketch_on(polar, "OnePolarSketch", offset=place((0, 60, 0)))
add(disk, circle((0, 0), 20))
base = pad(polar, disk, 6, "OnePolarDisk")
sketch = sketch_on_face(polar, "OnePolarHoleSketch", base, (0, 0, 1), 6)
add(sketch, circle((14, 60), 2))
hole = pocket(polar, sketch, "OnePolarHole", through_all=True)
axis = polar.newObject("PartDesign::Line", "OnePolarAxis")
axis.MapMode = "Deactivated"
axis.Placement = place((0, 60, 0))
hide(axis)
multi(polar, "OnePolarHoles", [hole], [
    ("PolarPattern", {"Axis": (axis, [""]), "Angle": 360, "Occurrences": 5}),
])

doc.recompute()
for container in (plate, mirrored, polar):
    for obj in reversed(container.Group):
        if obj.TypeId == "PartDesign::MultiTransform":
            container.Tip = obj
            break
