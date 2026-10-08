# SPDX-License-Identifier: MIT
# Helices that widen, one per Body (the profiles on the XZ plane, turned
# about their sketch's V axis). FreeCAD keeps the profile (nearly) in planes
# through the axis as it moves out; Mitcad's helix with a growth per turn
# does the same:
#
# - Conical: a circle of radius 1 at x = 8, pitch 5, 20 high, widening at
#   10 degrees.
# - Growing: the same, 4 turns in 20, growing 1 mm a turn.
# - Narrowing: a circle at x = 10, reversed (downwards), pitch 4, 12 high,
#   narrowing at 8 degrees.
# - GrowingLeft: left-handed, 3 turns in 15, growing 0.5 mm a turn.
#
# FreeCAD 0.21 builds a growing helix along an auxiliary spine, 1.0 and 1.1
# (and Mitcad) by the Frenet frame alone: their shapes differ by about
# 1e-6, so a growing helix of 0.21 may fall back.


def helix(container, name, profile, axis, **options):
    feature = container.newObject("PartDesign::AdditiveHelix", name)
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


conical = body("Conical")
profile = round_profile(conical, "ConicalProfile", 8, 0)
helix(conical, "ConicalHelix", profile, (profile, ["V_Axis"]), Pitch=5, Height=20, Angle=10)

growing = body("Growing")
profile = round_profile(growing, "GrowingProfile", 8, 40)
helix(growing, "GrowingHelix", profile, (profile, ["V_Axis"]), Mode="height-turns-growth",
      Height=20, Turns=4, Growth=1)

narrowing = body("Narrowing")
profile = round_profile(narrowing, "NarrowingProfile", 10, 80)
helix(narrowing, "NarrowingHelix", profile, (profile, ["V_Axis"]), Pitch=4, Height=12, Angle=-8,
      Reversed=True)

left = body("GrowingLeft")
profile = round_profile(left, "GrowingLeftProfile", 8, 120)
helix(left, "GrowingLeftHelix", profile, (profile, ["V_Axis"]), Mode="height-turns-growth",
      Height=15, Turns=3, Growth=0.5, LeftHanded=True)
