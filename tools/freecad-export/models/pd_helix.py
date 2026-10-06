# SPDX-License-Identifier: MIT
# Helices, one per Body (the profiles on the XZ plane, turned about the
# Z axis):
#
# - Spring: a circle of radius 1 at x = 8 turned about its sketch's V axis,
#   pitch 5, 20 high.
# - LeftSpring: the same left-handed, pitch 4 and 3.5 turns.
# - Downward: the same reversed, 12 high in 3 turns.
# - SquareWire: a 1.5 mm square at x = 10 about the Body's Z axis, pitch 4,
#   12 high.
# - Thread: a 6 mm rod with a triangular groove cut along a helix
#   (SubtractiveHelix), pitch 2, 16 high.
#
# Mitcad expects: no fallback


def helix(container, name, profile, axis, kind="AdditiveHelix", **options):
    feature = container.newObject("PartDesign::" + kind, name)
    feature.Profile = profile
    feature.ReferenceAxis = axis
    for key, value in options.items():
        setattr(feature, key, value)
    hide(profile)
    return feature


def round_profile(container, name, x, z, radius=1.0):
    sketch = sketch_on(container, name, plane="XZ_Plane")
    add(sketch, circle((x, z), radius))
    return sketch


spring = body("Spring")
profile = round_profile(spring, "SpringProfile", 8, 0)
helix(spring, "SpringHelix", profile, (profile, ["V_Axis"]), Pitch=5, Height=20)

left = body("LeftSpring")
profile = round_profile(left, "LeftSpringProfile", 8, 30)
helix(left, "LeftSpringHelix", profile, (profile, ["V_Axis"]), Mode="pitch-turns-angle", Pitch=4,
      Turns=3.5, LeftHanded=True)

down = body("Downward")
profile = round_profile(down, "DownwardProfile", 8, -10)
helix(down, "DownwardHelix", profile, (profile, ["V_Axis"]), Mode="height-turns-angle", Height=12,
      Turns=3, Reversed=True)

square = body("SquareWire")
sketch = sketch_on(square, "SquareWireProfile", plane="XZ_Plane")
rectangle(sketch, 10, 60, 1.5, 1.5)
helix(square, "SquareWireHelix", sketch, (origin_feature(square, "Z_Axis"), [""]), Pitch=4, Height=12)

threaded = body("Thread")
rod = sketch_on(threaded, "ThreadRodSketch", offset=place((40, 0, 0)))
add(rod, circle((0, 0), 6))
pad(threaded, rod, 20, "ThreadRod")
groove = sketch_on(threaded, "ThreadProfile", plane="XZ_Plane")
polyline(groove, [(45.2, 2), (46.5, 2.8), (46.5, 1.2)], closed=True)
axis = threaded.newObject("PartDesign::Line", "ThreadAxis")
axis.MapMode = "Deactivated"
axis.Placement = place((40, 0, 0))
hide(axis)
helix(threaded, "ThreadGroove", groove, (axis, [""]), kind="SubtractiveHelix", Pitch=2, Height=16)
