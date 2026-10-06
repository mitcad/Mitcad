# SPDX-License-Identifier: MIT
# Part workbench primitives Mitcad builds from sections, at placements of
# their own (each a body of its own):
#
# - PartCylinder: radius 5, 12 high, 90 degrees.
# - PartCone: radii 6 and 0, 9 high, 200 degrees, tilted.
# - PartSphere: radius 7, from -60 to 30 degrees of latitude, 300 degrees.
# - PartTorus: radii 12 and 3, the section from -150 to 30 degrees, 90
#   degrees round.
# - Prism: an octagonal prism, circumradius 6, 5 high, turned.
# - Wedge: from 12 x 8 at its base to 4 x 8 at its top, moved.
# - Ellipsoid: radii 5 (polar), 8 and 3.
#
# Mitcad expects: no fallback


def primitive(kind, name, at, **sizes):
    feature = named(doc.addObject("Part::" + kind, name), name)
    feature.Placement = at
    for key, value in sizes.items():
        setattr(feature, key, value)
    return feature


primitive("Cylinder", "PartCylinder", place((0, 0, 0)), Radius=5, Height=12, Angle=90)
primitive("Cone", "PartCone", place((30, 0, 0), (0, 1, 0), 20), Radius1=6, Radius2=0, Height=9, Angle=200)
primitive("Sphere", "PartSphere", place((60, 0, 0)), Radius=7, Angle1=-60, Angle2=30, Angle3=300)
primitive("Torus", "PartTorus", place((0, 40, 0), (1, 0, 0), 90), Radius1=12, Radius2=3,
          Angle1=-150, Angle2=30, Angle3=90)
primitive("Prism", "Prism", place((40, 40, 0), (0, 0, 1), 10), Polygon=8, Circumradius=6, Height=5)
primitive("Wedge", "Wedge", place((70, 40, 3)), Xmin=0, Xmax=12, Ymin=0, Ymax=6, Zmin=0, Zmax=8,
          X2min=4, X2max=8, Z2min=0, Z2max=8)
primitive("Ellipsoid", "Ellipsoid", place((0, 80, 0)), Radius1=5, Radius2=8, Radius3=3)
