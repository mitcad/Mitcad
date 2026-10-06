# SPDX-License-Identifier: MIT
# Parts of ellipsoids (FreeCAD scales a part of a sphere of Radius2), one
# per Body or Part object:
#
# - UpperHalf: an additive ellipsoid, radii 6 (polar), 10 and 8, from the
#   equator up.
# - Wedged: an additive ellipsoid, radii 5 and 8, latitudes -30 to 60
#   degrees, 270 degrees round, turned.
# - Carved: a block with a subtractive ellipsoid, radii 4 and 7, the
#   lower half, 180 degrees round.
# - PartEllipsoid: a Part ellipsoid, radii 4, 6 and 5, latitudes -90 to
#   45 degrees, 120 degrees round, moved and turned.
#
# Mitcad expects: no fallback


def primitive(container, kind, name, at, **sizes):
    feature = container.newObject("PartDesign::" + kind, name)
    feature.MapMode = "Deactivated"
    feature.Placement = at
    for key, value in sizes.items():
        setattr(feature, key, value)
    return feature


primitive(body("UpperHalf"), "AdditiveEllipsoid", "AddUpperHalf", place((0, 0, 0)),
          Radius1=6, Radius2=10, Radius3=8, Angle1=0, Angle2=90)
primitive(body("Wedged"), "AdditiveEllipsoid", "AddWedged", place((30, 0, 5), (1, 1, 0), 30),
          Radius1=5, Radius2=8, Angle1=-30, Angle2=60, Angle3=270)
carved = body("Carved")
block(carved, "CarvedBase", 30, 20, 10, 50, -10)
primitive(carved, "SubtractiveEllipsoid", "CarvedEllipsoid", place((65, 0, 10)),
          Radius1=4, Radius2=7, Angle1=-90, Angle2=0, Angle3=180)

part = named(doc.addObject("Part::Ellipsoid", "PartEllipsoid"), "PartEllipsoid")
part.Placement = place((0, 40, 0), (0, 0, 1), 30)
part.Radius1, part.Radius2, part.Radius3 = 4, 6, 5
part.Angle1, part.Angle2, part.Angle3 = -90, 45, 120
