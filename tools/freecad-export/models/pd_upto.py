# SPDX-License-Identifier: MIT
# Revolutions, grooves, pads and pockets up to faces and shapes, one per
# Body (FreeCAD 1.0 and later: "up to shape", the revolutions' types):
#
# - RevolutionFace: a block, and a rectangle on the XZ plane turned about
#   the Z axis up to the block's face in the YZ plane.
# - PocketShape: a stepped block, the high step pocketed down to the low
#   step's top face (up to a shape of one face).
# - PadShape: a block, and a circle 30 mm up padded down to the Body's
#   shape (up to a shape without shapes).
#
# (A revolution up to the first face fails in these versions, and a groove
# up to a face cuts a part of the turn in 1.0 and empties the Body in 1.1;
# Mitcad translates grooves as revolutions.)
#
# Mitcad expects: no fallback

if not at_least(1, 0):
    raise Unsupported("up to shape and the revolutions' types are new in FreeCAD 1.0")


def turned_rectangle(container, name, x0, x1, z0, z1, feature="PartDesign::Revolution"):
    sketch = sketch_on(container, name + "Sketch", plane="XZ_Plane")
    rectangle(sketch, x0, z0, x1 - x0, z1 - z0)
    revolved = container.newObject(feature, name)
    revolved.Profile = sketch
    revolved.ReferenceAxis = (origin_feature(container, "Z_Axis"), [""])
    hide(sketch)
    return revolved


# A block in the second quadrant, its face at x = 0 facing +X.
revolved = body("RevolutionFace")
stop = block(revolved, "RevolutionFaceStop", 20, 20, 10, -20, 0)
doc.recompute()
feature = turned_rectangle(revolved, "RevolutionFaceTurn", 5, 8, 0, 5)
feature.Type = "UpToFace"
feature.UpToFace = (stop, [face_index(stop.Shape, (1, 0, 0))])


stepped = body("PocketShape")
low = block(stepped, "PocketShapeLow", 30, 20, 10, 40, 40)
high_sketch = sketch_on_face(stepped, "PocketShapeHighSketch", low, (0, 0, 1), 10)
rectangle(high_sketch, 40, 40, 15, 20)
high = pad(stepped, high_sketch, 10, "PocketShapeHigh")
doc.recompute()
step_face = face_index(high.Shape, (0, 0, 1), 10)
pocket_sketch = sketch_on_face(stepped, "PocketShapeSketch", high, (0, 0, 1), 20)
add(pocket_sketch, circle((47, 50), 3))
feature = pocket(stepped, pocket_sketch, "PocketShapePocket", length=5)
feature.Type = "UpToShape"
feature.UpToShape = [(high, [step_face])]

padded = body("PadShape")
base = block(padded, "PadShapeBase", 30, 20, 10, 80, 40)
sketch = sketch_on(padded, "PadShapeSketch", offset=place((0, 0, 30)))
add(sketch, circle((95, 50), 4))
feature = pad(padded, sketch, 5, "PadShapePad", reversed_=True)
feature.Type = "UpToShape"

doc.recompute()
for container in (revolved, stepped, padded):
    container.Tip = container.Group[-1]
