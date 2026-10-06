# SPDX-License-Identifier: MIT
# A MultiTransform with a Scaled transformation, which Mitcad does not
# translate (its patterns and mirrors copy features by rigid motions and
# reflections; the import takes FreeCAD's stored shape, a fallback):
#
# - Scaled: a boss on a plate, 3 along the X axis over 40 mm, then scaled
#   1, 1.25 and 1.5 times about its centre (one factor per copy).


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

doc.recompute()
for obj in reversed(scaled.Group):
    if obj.TypeId == "PartDesign::MultiTransform":
        scaled.Tip = obj
        break
