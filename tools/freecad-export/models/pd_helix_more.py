# SPDX-License-Identifier: MIT
# Helices that widen, one per Body (the profiles on the XZ plane, turned
# about their sketch's V axis). FreeCAD keeps the profile in planes through
# the axis as it moves out; Mitcad's helix is a screw motion, so these take
# FreeCAD's stored shapes (fallbacks):
#
# - Conical: a circle of radius 1 at x = 8, pitch 5, 20 high, widening at
#   10 degrees.
# - Growing: the same, 4 turns in 20, growing 1 mm a turn.


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
