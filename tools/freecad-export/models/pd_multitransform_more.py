# SPDX-License-Identifier: MIT
# MultiTransform features of other sequences, which Mitcad replays as
# patterns and mirrors of the one before (patterns of patterns), one per
# Body, each Body placed apart:
#
# - LinearPolar: a hole through a disk at x = 10, 2 along the X axis over
#   8 mm, then 4 round the Z axis.
# - PolarLinear: a boss on a plate at x = 10, 3 round the Z axis, then 2
#   along the Y axis over 12 mm.
# - MirrorLinear: a boss mirrored about the YZ plane, then 3 along the Y
#   axis over 20 mm.
# - LinearMirror: a hole 3 along the X axis over 20 mm, then mirrored about
#   the XZ plane.
# - PolarMirror: a hole 3 round the Z axis, then mirrored about the XZ
#   plane.
# - MirrorPolar: a hole mirrored about the XZ plane, then 4 round the Z
#   axis.
# - ThreeSteps: a hole 2 along X over 6 mm, 2 along Y over 6 mm, then 4
#   round Z.
# - TwoTurns: a pocket 5 deep in the top of a cube, 4 round the X axis,
#   then 2 round the Z axis.
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


def axis(container, role):
    return (origin_feature(container, role), [""])


def disk(container, name, radius, height):
    sketch = sketch_on(container, name + "Sketch")
    add(sketch, circle((0, 0), radius))
    return pad(container, sketch, height, name)


def hole(container, base, name, at, radius, top):
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), top)
    add(sketch, circle(at, radius))
    return pocket(container, sketch, name, through_all=True)


def boss(container, base, name, at, radius, top, height=4):
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), top)
    add(sketch, circle(at, radius))
    return pad(container, sketch, height, name)


bodies = []

linear_polar = body("LinearPolar", place((0, 0, 0)))
base = disk(linear_polar, "LinearPolarDisk", 30, 5)
original = hole(linear_polar, base, "LinearPolarHole", (10, 0), 2, 5)
multi(linear_polar, "LinearPolarHoles", [original], [
    ("LinearPattern", {"Direction": axis(linear_polar, "X_Axis"), "Length": 8, "Occurrences": 2}),
    ("PolarPattern", {"Axis": axis(linear_polar, "Z_Axis"), "Angle": 360, "Occurrences": 4}),
])
bodies.append(linear_polar)

polar_linear = body("PolarLinear", place((80, 0, 0)))
base = block(polar_linear, "PolarLinearPlate", 40, 45, 5, -20, -15)
original = boss(polar_linear, base, "PolarLinearBoss", (10, 0), 3, 5)
multi(polar_linear, "PolarLinearBosses", [original], [
    ("PolarPattern", {"Axis": axis(polar_linear, "Z_Axis"), "Angle": 360, "Occurrences": 3}),
    ("LinearPattern", {"Direction": axis(polar_linear, "Y_Axis"), "Length": 12, "Occurrences": 2}),
])
bodies.append(polar_linear)

mirror_linear = body("MirrorLinear", place((160, 0, 0)))
base = block(mirror_linear, "MirrorLinearPlate", 60, 40, 5, -30, -10)
original = boss(mirror_linear, base, "MirrorLinearBoss", (10, 0), 3, 5)
multi(mirror_linear, "MirrorLinearBosses", [original], [
    ("Mirrored", {"MirrorPlane": (origin_feature(mirror_linear, "YZ_Plane"), [""])}),
    ("LinearPattern", {"Direction": axis(mirror_linear, "Y_Axis"), "Length": 20, "Occurrences": 3}),
])
bodies.append(mirror_linear)

linear_mirror = body("LinearMirror", place((0, 80, 0)))
base = block(linear_mirror, "LinearMirrorPlate", 30, 20, 5, 0, -10)
original = hole(linear_mirror, base, "LinearMirrorHole", (5, 5), 1.5, 5)
multi(linear_mirror, "LinearMirrorHoles", [original], [
    ("LinearPattern", {"Direction": axis(linear_mirror, "X_Axis"), "Length": 20, "Occurrences": 3}),
    ("Mirrored", {"MirrorPlane": (origin_feature(linear_mirror, "XZ_Plane"), [""])}),
])
bodies.append(linear_mirror)

polar_mirror = body("PolarMirror", place((80, 80, 0)))
base = disk(polar_mirror, "PolarMirrorDisk", 25, 5)
original = hole(polar_mirror, base, "PolarMirrorHole", (12, 4), 1.5, 5)
multi(polar_mirror, "PolarMirrorHoles", [original], [
    ("PolarPattern", {"Axis": axis(polar_mirror, "Z_Axis"), "Angle": 360, "Occurrences": 3}),
    ("Mirrored", {"MirrorPlane": (origin_feature(polar_mirror, "XZ_Plane"), [""])}),
])
bodies.append(polar_mirror)

mirror_polar = body("MirrorPolar", place((160, 80, 0)))
base = disk(mirror_polar, "MirrorPolarDisk", 25, 5)
original = hole(mirror_polar, base, "MirrorPolarHole", (12, 4), 1.5, 5)
multi(mirror_polar, "MirrorPolarHoles", [original], [
    ("Mirrored", {"MirrorPlane": (origin_feature(mirror_polar, "XZ_Plane"), [""])}),
    ("PolarPattern", {"Axis": axis(mirror_polar, "Z_Axis"), "Angle": 360, "Occurrences": 4}),
])
bodies.append(mirror_polar)

three = body("ThreeSteps", place((0, 160, 0)))
base = block(three, "ThreeStepsPlate", 40, 40, 5, -20, -20)
original = hole(three, base, "ThreeStepsHole", (6, 6), 1.5, 5)
multi(three, "ThreeStepsHoles", [original], [
    ("LinearPattern", {"Direction": axis(three, "X_Axis"), "Length": 6, "Occurrences": 2}),
    ("LinearPattern", {"Direction": axis(three, "Y_Axis"), "Length": 6, "Occurrences": 2}),
    ("PolarPattern", {"Axis": axis(three, "Z_Axis"), "Angle": 360, "Occurrences": 4}),
])
bodies.append(three)

turns = body("TwoTurns", place((80, 160, 0)))
sketch = sketch_on(turns, "TwoTurnsCubeSketch")
rectangle(sketch, -15, -15, 30, 30)
cube = pad(turns, sketch, 30, "TwoTurnsCube")
sides(cube, "symmetric")
sketch = sketch_on_face(turns, "TwoTurnsPocketSketch", cube, (0, 0, 1), 15)
add(sketch, circle((5, 5), 2))
original = pocket(turns, sketch, "TwoTurnsPocket", length=5)
multi(turns, "TwoTurnsPockets", [original], [
    ("PolarPattern", {"Axis": axis(turns, "X_Axis"), "Angle": 360, "Occurrences": 4}),
    ("PolarPattern", {"Axis": axis(turns, "Z_Axis"), "Angle": 360, "Occurrences": 2}),
])
bodies.append(turns)

doc.recompute()
for container in bodies:
    for obj in reversed(container.Group):
        if obj.TypeId == "PartDesign::MultiTransform":
            container.Tip = obj
            break
