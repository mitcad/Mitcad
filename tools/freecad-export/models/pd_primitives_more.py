# SPDX-License-Identifier: MIT
# PartDesign primitives Mitcad builds from sections, one per Body unless
# said (placed by their placements, not attached):
#
# - PartCylinder: an additive cylinder, radius 5, 10 high, 270 degrees.
# - SkewCylinder: an additive cylinder, radius 4, 10 high, skewed 15 and
#   -10 degrees.
# - PartCone: an additive cone, radii 6 and 2, 8 high, 120 degrees.
# - PartSphere: an additive sphere, radius 6, from -30 to 60 degrees of
#   latitude, 270 degrees round.
# - Dome: an additive sphere, radius 5, its upper half.
# - PartTorus: an additive torus, radii 10 and 2, the outer half of its
#   section, 180 degrees round.
# - SectorTorus: an additive torus, a 120-degree sector of its section, all
#   the way round.
# - Prism: an additive hexagonal prism, circumradius 5, 8 high.
# - SkewPrism: an additive pentagonal prism, skewed 10 and 20 degrees.
# - Wedge: an additive wedge, 10 x 10 at its base to 6 x 6 at its top.
# - Pyramid: an additive wedge whose top is a point.
# - Spheroid: an additive ellipsoid, radii 4 (polar) and 6.
# - Ellipsoid: an additive ellipsoid, radii 3, 6 and 4, turned.
# - Carved: a block with a subtractive prism, wedge and part of a cone.
#
# Mitcad expects: no fallback


def primitive(container, kind, name, at, **sizes):
    feature = container.newObject("PartDesign::" + kind, name)
    feature.MapMode = "Deactivated"
    feature.Placement = at
    for key, value in sizes.items():
        setattr(feature, key, value)
    return feature


primitive(body("PartCylinder"), "AdditiveCylinder", "AddPartCylinder", place((0, 0, 0)),
          Radius=5, Height=10, Angle=270)
primitive(body("SkewCylinder"), "AdditiveCylinder", "AddSkewCylinder", place((30, 0, 0), (0, 0, 1), 20),
          Radius=4, Height=10, FirstAngle=15, SecondAngle=-10)
primitive(body("PartCone"), "AdditiveCone", "AddPartCone", place((60, 0, 0), (1, 0, 0), 30),
          Radius1=6, Radius2=2, Height=8, Angle=120)
primitive(body("PartSphere"), "AdditiveSphere", "AddPartSphere", place((90, 0, 0)),
          Radius=6, Angle1=-30, Angle2=60, Angle3=270)
primitive(body("Dome"), "AdditiveSphere", "AddDome", place((120, 0, 0), (0, 1, 0), 90),
          Radius=5, Angle1=0)
primitive(body("PartTorus"), "AdditiveTorus", "AddPartTorus", place((0, 40, 0)),
          Radius1=10, Radius2=2, Angle1=-90, Angle2=90, Angle3=180)
primitive(body("SectorTorus"), "AdditiveTorus", "AddSectorTorus", place((40, 40, 0), (0, 0, 1), 45),
          Radius1=10, Radius2=3, Angle1=0, Angle2=120)
primitive(body("Prism"), "AdditivePrism", "AddPrism", place((80, 40, 0)),
          Polygon=6, Circumradius=5, Height=8)
primitive(body("SkewPrism"), "AdditivePrism", "AddSkewPrism", place((110, 40, 0), (0, 1, 0), 10),
          Polygon=5, Circumradius=4, Height=10, FirstAngle=10, SecondAngle=20)
primitive(body("Wedge"), "AdditiveWedge", "AddWedge", place((0, 80, 0)),
          Xmin=0, Xmax=10, Ymin=0, Ymax=10, Zmin=0, Zmax=10, X2min=2, X2max=8, Z2min=2, Z2max=8)
primitive(body("Pyramid"), "AdditiveWedge", "AddPyramid", place((30, 80, 0), (1, 0, 0), 90),
          Xmin=-5, Xmax=5, Ymin=0, Ymax=8, Zmin=-5, Zmax=5, X2min=0, X2max=0, Z2min=0, Z2max=0)
primitive(body("Spheroid"), "AdditiveEllipsoid", "AddSpheroid", place((60, 80, 0)),
          Radius1=4, Radius2=6)
primitive(body("Ellipsoid"), "AdditiveEllipsoid", "AddEllipsoid", place((90, 80, 5), (1, 1, 0), 30),
          Radius1=3, Radius2=6, Radius3=4)
carved = body("Carved")
block(carved, "CarvedBase", 40, 20, 10, 0, 120)
primitive(carved, "SubtractivePrism", "CarvedPrism", place((8, 130, 4)),
          Polygon=6, Circumradius=3, Height=8)
primitive(carved, "SubtractiveWedge", "CarvedWedge", place((15, 125, 10), (1, 0, 0), -90),
          Xmin=0, Xmax=8, Ymin=0, Ymax=5, Zmin=0, Zmax=10, X2min=2, X2max=6, Z2min=0, Z2max=10)
primitive(carved, "SubtractiveCone", "CarvedCone", place((32, 130, 4)),
          Radius1=4, Radius2=2, Height=8, Angle=180)
