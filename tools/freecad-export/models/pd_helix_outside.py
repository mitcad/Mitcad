# SPDX-License-Identifier: MIT
# Subtractive helices cutting outside the profile (FreeCAD's Outside: the
# body keeps only what the helix sweeps through; Mitcad: a helix that
# intersects), one per Body:
#
# - OutsideRod: a 6 mm rod, 20 long, and a 2 x 1.5 rectangle reaching
#   1 mm into it, pitch 4, 16 high: a helical ridge is left.
# - OutsideCone: a 10 mm rod and a circle of radius 1.5 on its surface,
#   pitch 6, 18 high, narrowing at 5 degrees.
#
# Both fall back: FreeCAD refines the intersection (Refine), which moves
# its stored shape by about 1e-5 of the volume and 1e-4 of its size in
# the centre from the plain intersection of its own helix and support
# (measured in FreeCAD 1.1); 0.21's narrowing helix differs again.


def helix(container, name, profile, axis, **options):
    feature = container.newObject("PartDesign::SubtractiveHelix", name)
    feature.Profile = profile
    feature.ReferenceAxis = axis
    for key, value in options.items():
        setattr(feature, key, value)
    hide(profile)
    return feature


def rod(container, name, x, radius, length):
    sketch = sketch_on(container, name + "Sketch", offset=place((x, 0, 0)))
    add(sketch, circle((0, 0), radius))
    return pad(container, sketch, length, name)


def axis_at(container, name, x):
    line = container.newObject("PartDesign::Line", name)
    line.MapMode = "Deactivated"
    line.Placement = place((x, 0, 0))
    hide(line)
    return line


ridge = body("OutsideRod")
rod(ridge, "OutsideRodBase", 0, 6, 20)
profile = sketch_on(ridge, "OutsideRodProfile", plane="XZ_Plane")
rectangle(profile, 5, 2, 2, 1.5)
helix(ridge, "OutsideRodHelix", profile, (axis_at(ridge, "OutsideRodAxis", 0), [""]), Pitch=4,
      Height=16, Outside=True)

cone = body("OutsideCone")
rod(cone, "OutsideConeBase", 40, 10, 20)
profile = sketch_on(cone, "OutsideConeProfile", plane="XZ_Plane")
add(profile, circle((50, 1), 1.5))
helix(cone, "OutsideConeHelix", profile, (axis_at(cone, "OutsideConeAxis", 40), [""]), Pitch=6,
      Height=18, Angle=-5, Outside=True)
