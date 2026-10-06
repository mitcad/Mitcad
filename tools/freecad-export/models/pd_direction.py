# SPDX-License-Identifier: MIT
# Pads and pockets along directions off their sketches' normals, and Part
# extrusions along such directions, one option per Body or object:
#
# - Custom: a 20 x 10 rectangle padded 10 mm along (1, 0, 2), the length
#   along the sketch's normal.
# - CustomAlong: the same, the length along the direction.
# - CustomTwo: two lengths, 8 and 3, along (0, 1, 1).
# - CustomSymmetric: 12 mm symmetric about the sketch, along (1, 1, 3).
# - PocketCustom: a block with a circle pocketed 6 mm along (1, 0, -2).
# - Datum: a pad along a datum line tilted 30 degrees about Y.
# - Extruded: a Part extrusion of a rectangle sketch along (1, 1, 2).
# - ExtrudedSymmetric: 10 mm along (0, 1, 1), symmetric.
# - ExtrudedEdge: along the edge of a Part line from (0, 0, 0) to
#   (2, -1, 4), 9 mm.
#
# Mitcad expects: no fallback


def custom(container, name, x, y, direction, length=10, along_normal=True, **options):
    sketch = sketch_on(container, name + "Sketch")
    rectangle(sketch, x, y, 20, 10)
    feature = pad(container, sketch, length, name)
    feature.UseCustomVector = True
    feature.Direction = V(*direction)
    feature.AlongSketchNormal = along_normal
    for key, value in options.items():
        if key == "sides":
            sides(feature, value)
        else:
            setattr(feature, key, value)
    return feature


custom(body("Custom"), "CustomPad", 0, 0, (1, 0, 2))
custom(body("CustomAlong"), "CustomAlongPad", 40, 0, (1, 0, 2), along_normal=False)
two = custom(body("CustomTwo"), "CustomTwoPad", 80, 0, (0, 1, 1), length=8, sides="two")
two.Length2 = 3
custom(body("CustomSymmetric"), "CustomSymmetricPad", 120, 0, (1, 1, 3), length=12, sides="symmetric")

pocketed = body("PocketCustom")
base = block(pocketed, "PocketCustomBase", 30, 20, 10, 0, 40)
sketch = sketch_on_face(pocketed, "PocketCustomSketch", base, (0, 0, 1), 10)
add(sketch, circle((15, 50), 4))
feature = pocket(pocketed, sketch, "PocketCustomPocket", length=6)
feature.UseCustomVector = True
feature.Direction = V(1, 0, -2)

along_datum = body("Datum")
line_datum = along_datum.newObject("PartDesign::Line", "DatumAxis")
line_datum.MapMode = "Deactivated"
line_datum.Placement = place((40, 40, 0), (0, 1, 0), 30)
sketch = sketch_on(along_datum, "DatumSketch")
rectangle(sketch, 40, 40, 20, 10)
feature = pad(along_datum, sketch, 10, "DatumPad")
feature.ReferenceAxis = (line_datum, [""])
hide(line_datum)


def profile(name, x, y):
    sketch = doc.addObject("Sketcher::SketchObject", name)
    sketch.Label = name
    rectangle(sketch, x, y, 20, 10)
    hide(sketch)
    return sketch


def extrusion(name, base, direction, length, **options):
    feature = named(doc.addObject("Part::Extrusion", name), name)
    feature.Base = base
    feature.DirMode = "Custom"
    feature.Dir = V(*direction)
    feature.Solid = True
    feature.LengthFwd = length
    for key, value in options.items():
        setattr(feature, key, value)
    return feature


extrusion("Extruded", profile("ExtrudedProfile", 0, 80), (1, 1, 2), 10)
extrusion("ExtrudedSymmetric", profile("ExtrudedSymmetricProfile", 40, 80), (0, 1, 1), 10, Symmetric=True)
edge = named(doc.addObject("Part::Line", "EdgeLine"), "EdgeLine")
edge.X1, edge.Y1, edge.Z1 = 0, 0, 0
edge.X2, edge.Y2, edge.Z2 = 2, -1, 4
hide(edge)
along_edge = extrusion("ExtrudedEdge", profile("ExtrudedEdgeProfile", 80, 80), (0, 0, 1), 9)
along_edge.DirMode = "Edge"
along_edge.DirLink = (edge, ["Edge1"])
