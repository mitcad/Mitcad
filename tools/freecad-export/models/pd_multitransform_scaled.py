# SPDX-License-Identifier: MIT
# MultiTransforms with a Scaled transformation (Mitcad: a pattern with its
# `scale`, the copies scaled about the original's centre of mass):
#
# - Scaled: a boss on a plate, 3 along the X axis over 40 mm, then scaled
#   1, 1.25 and 1.5 times about its centre (one factor per copy).
# - ScaledPolar: a boss on a disc, 4 round the Z axis, then scaled 1 to
#   1.6 times (each copy reaching into the disc).
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


scaled = body("Scaled")
base = block(scaled, "ScaledPlate", 60, 20, 5)
sketch = sketch_on_face(scaled, "ScaledBossSketch", base, (0, 0, 1), 5)
add(sketch, circle((8, 10), 3))
original = pad(scaled, sketch, 4, "ScaledBoss")
multi(scaled, "ScaledBosses", [original], [
    ("LinearPattern", {"Direction": (origin_feature(scaled, "X_Axis"), [""]), "Length": 40,
                       "Occurrences": 3}),
    ("Scaled", {"Factor": 1.5, "Occurrences": 3}),
])

polar = body("ScaledPolar")
disc = polar.newObject("Sketcher::SketchObject", "ScaledDiscSketch")
attach(disc, [(origin_feature(polar, "XY_Plane"), "")])
add(disc, circle((0, 40), 15))
disc_pad = pad(polar, disc, 3, "ScaledDisc")
sketch = sketch_on_face(polar, "ScaledPolarBossSketch", disc_pad, (0, 0, 1), 3)
add(sketch, circle((10, 40), 2.5))
original = pad(polar, sketch, 5, "ScaledPolarBoss")
axis = polar.newObject("PartDesign::Line", "ScaledPolarAxis")
axis.MapMode = "Deactivated"
axis.Placement = place((0, 40, 0))
hide(axis)
multi(polar, "ScaledPolarBosses", [original], [
    ("PolarPattern", {"Axis": (axis, [""]), "Angle": 360, "Occurrences": 4}),
    ("Scaled", {"Factor": 1.6, "Occurrences": 4}),
])

doc.recompute()
for container in (scaled, polar):
    for obj in reversed(container.Group):
        if obj.TypeId == "PartDesign::MultiTransform":
            container.Tip = obj
            break
