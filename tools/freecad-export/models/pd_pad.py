# SPDX-License-Identifier: MIT
# Pads, one option per Body (each Body's first feature is a 30 x 20 block,
# 10 mm, unless the option is the first pad's own):
#
# - Plain: the block itself.
# - Reversed: padded down from the XY plane.
# - Symmetric: 12 mm in all about the XY plane (Midplane; 1.1 SideType).
# - TwoSides: 10 mm up and 4 mm down (TwoLengths; 1.1 SideType).
# - Tapered: tapered 8 degrees.
# - Holed: a rectangle with a circle inside it (a hole through the pad).
#   (An island inside the hole is left out: FreeCAD 0.21 fails on it, 1.0
#   leaves it out and 1.1 pads it as a second solid.)
# - OnTop: a cylinder padded on a sketch on the block's top face.
# - UpToFace: a cylinder from a sketch 30 mm up, padded down to the
#   block's top face.
# - UpToFirst: the same down to the first face it meets.
#
# Mitcad expects: no fallback

plain = body("Plain")
block(plain, "PlainPad", 30, 20, 10)

reversed_body = body("Reversed")
sketch = sketch_on(reversed_body, "ReversedSketch")
rectangle(sketch, 40, 0, 30, 20)
pad(reversed_body, sketch, 10, "ReversedPad", reversed_=True)

symmetric = body("Symmetric")
sketch = sketch_on(symmetric, "SymmetricSketch")
rectangle(sketch, 80, 0, 30, 20)
feature = pad(symmetric, sketch, 12, "SymmetricPad")
sides(feature, "symmetric")

two = body("TwoSides")
sketch = sketch_on(two, "TwoSidesSketch")
rectangle(sketch, 120, 0, 30, 20)
feature = pad(two, sketch, 10, "TwoSidesPad")
sides(feature, "two")
feature.Length2 = 4

tapered = body("Tapered")
sketch = sketch_on(tapered, "TaperedSketch")
rectangle(sketch, 160, 0, 30, 20)
feature = pad(tapered, sketch, 10, "TaperedPad")
feature.TaperAngle = 8

holed = body("Holed")
sketch = sketch_on(holed, "HoledSketch")
rectangle(sketch, 0, 40, 30, 20)
add(sketch, circle((15, 50), 8))
pad(holed, sketch, 10, "HoledPad")

on_top = body("OnTop")
base = block(on_top, "OnTopBase", 30, 20, 10, 40, 40)
sketch = sketch_on_face(on_top, "OnTopSketch", base, (0, 0, 1), 10)
add(sketch, circle((55, 50), 5))
pad(on_top, sketch, 6, "OnTopPad")

up_to_face = body("UpToFace")
base = block(up_to_face, "UpToFaceBase", 30, 20, 10, 80, 40)
sketch = sketch_on(up_to_face, "UpToFaceSketch", offset=place((0, 0, 30)))
add(sketch, circle((95, 50), 5))
feature = pad(up_to_face, sketch, 5, "UpToFacePad", reversed_=True)
doc.recompute()
feature.Type = "UpToFace"
feature.UpToFace = (base, [face_index(base.Shape, (0, 0, 1), 10)])

up_to_first = body("UpToFirst")
base = block(up_to_first, "UpToFirstBase", 30, 20, 10, 120, 40)
sketch = sketch_on(up_to_first, "UpToFirstSketch", offset=place((0, 0, 30)))
add(sketch, circle((135, 50), 5))
feature = pad(up_to_first, sketch, 5, "UpToFirstPad", reversed_=True)
feature.Type = "UpToFirst"
