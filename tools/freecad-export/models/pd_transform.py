# SPDX-License-Identifier: MIT
# Mirrored, LinearPattern and PolarPattern, one option per Body:
#
# - Mirrored: a boss on a block mirrored about the Body's YZ plane.
# - MirroredSketch: the same about the V axis of the block's sketch.
# - Linear: a hole pocketed through a block, 4 along the sketch's H axis
#   over 40 mm.
# - LinearBoss: a boss padded on a block, 3 along the Body's X axis over
#   30 mm, reversed.
# - Polar: a hole through a disk, 6 round the sketch's normal (360 degrees).
# - PolarPartial: the same, 3 over 90 degrees about the Body's Z axis.
#
# Mitcad expects: no fallback


def boss(container, base, name, at, radius=3, height=5, top=10):
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), top)
    add(sketch, circle(at, radius))
    return pad(container, sketch, height, name)


def through_hole(container, base, name, at, radius=2, top=10):
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), top)
    add(sketch, circle(at, radius))
    return pocket(container, sketch, name, through_all=True)


# The originals come first: a transform made before them would be their
# base feature.
mirrored = body("Mirrored")
base = block(mirrored, "MirroredBase", 30, 20, 10, -15, 0)
original = boss(mirrored, base, "MirroredOriginal", (8, 10))
feature = mirrored.newObject("PartDesign::Mirrored", "MirroredBoss")
feature.Originals = [original]
feature.MirrorPlane = (origin_feature(mirrored, "YZ_Plane"), [""])

by_sketch = body("MirroredSketch")
base = block(by_sketch, "MirroredSketchBase", 30, 20, 10, -15, 40)
original = boss(by_sketch, base, "MirroredSketchOriginal", (8, 50))
feature = by_sketch.newObject("PartDesign::Mirrored", "MirroredSketchBoss")
feature.Originals = [original]
feature.MirrorPlane = (by_sketch.Group[0], ["V_Axis"])

linear = body("Linear")
base = block(linear, "LinearBase", 60, 20, 10, 40, 0)
original = through_hole(linear, base, "LinearHole", (48, 10))
feature = linear.newObject("PartDesign::LinearPattern", "LinearHoles")
feature.Originals = [original]
feature.Direction = (linear.Group[0], ["H_Axis"])
feature.Length = 40
feature.Occurrences = 4

boss_body = body("LinearBoss")
base = block(boss_body, "LinearBossBase", 60, 20, 10, 40, 40)
original = boss(boss_body, base, "LinearBossOriginal", (92, 50))
feature = boss_body.newObject("PartDesign::LinearPattern", "LinearBosses")
feature.Originals = [original]
feature.Direction = (origin_feature(boss_body, "X_Axis"), [""])
feature.Reversed = True
feature.Length = 30
feature.Occurrences = 3

# The disks lie about the Z axis, 30 mm above and below the blocks.
polar = body("Polar")
disk = sketch_on(polar, "PolarSketch", offset=place((0, 0, 30)))
add(disk, circle((0, 0), 20))
base = pad(polar, disk, 8, "PolarDisk")
original = through_hole(polar, base, "PolarHole", (14, 0), top=38)
feature = polar.newObject("PartDesign::PolarPattern", "PolarHoles")
feature.Originals = [original]
feature.Axis = (disk, ["N_Axis"])
feature.Angle = 360
feature.Occurrences = 6

partial = body("PolarPartial")
disk = sketch_on(partial, "PolarPartialSketch", offset=place((0, 0, -30)))
add(disk, circle((0, 0), 20))
base = pad(partial, disk, 8, "PolarPartialDisk")
original = through_hole(partial, base, "PolarPartialHole", (14, 0), top=-22)
feature = partial.newObject("PartDesign::PolarPattern", "PolarPartialHoles")
feature.Originals = [original]
feature.Axis = (origin_feature(partial, "Z_Axis"), [""])
feature.Angle = 90
feature.Occurrences = 3

# A transformation made by a script does not become its Body's tip.
doc.recompute()
for container in (mirrored, by_sketch, linear, boss_body, polar, partial):
    container.Tip = container.Group[-1]
