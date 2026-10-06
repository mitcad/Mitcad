# SPDX-License-Identifier: MIT
# PartDesign primitives, one per Body unless said (placed by their
# placements, not attached):
#
# - Box: an additive box 20 x 10 x 5.
# - Cylinder: an additive cylinder, radius 5, 10 high, lying along X.
# - Sphere: an additive sphere, radius 6.
# - Torus: an additive torus, radii 10 and 2.
# - Cone: an additive cone, radii 6 and 2, 8 high.
# - Carved: a block with a subtractive box and a subtractive cylinder.
#
# Mitcad expects: no fallback


def primitive(container, kind, name, at, **sizes):
    feature = container.newObject("PartDesign::" + kind, name)
    feature.MapMode = "Deactivated"
    feature.Placement = at
    for key, value in sizes.items():
        setattr(feature, key, value)
    return feature


primitive(body("Box"), "AdditiveBox", "AddBox", place((0, 0, 0)), Length=20, Width=10, Height=5)
primitive(body("Cylinder"), "AdditiveCylinder", "AddCylinder", place((40, 5, 5), (0, 1, 0), 90),
          Radius=5, Height=10)
primitive(body("Sphere"), "AdditiveSphere", "AddSphere", place((80, 5, 6)), Radius=6)
primitive(body("Torus"), "AdditiveTorus", "AddTorus", place((120, 5, 2)), Radius1=10, Radius2=2)
primitive(body("Cone"), "AdditiveCone", "AddCone", place((160, 5, 0)), Radius1=6, Radius2=2, Height=8)
carved = body("Carved")
block(carved, "CarvedBase", 30, 20, 10, 0, 40)
primitive(carved, "SubtractiveBox", "CarvedBox", place((5, 45, 6)), Length=8, Width=10, Height=6)
primitive(carved, "SubtractiveCylinder", "CarvedCylinder", place((22, 50, -1)), Radius=3, Height=12)
