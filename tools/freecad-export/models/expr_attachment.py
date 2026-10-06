# SPDX-License-Identifier: MIT
# Attachment offsets bound to expressions (Mitcad: construction planes
# offset by them):
#
# - Spreadsheet: Lift (=5 mm).
# - Stack: a 30 x 20 x 10 block; a datum plane on the XY plane offset
#   10 mm + Lift, with a circle on it padded Lift down to the block; a
#   sketch on the block's top face offset Lift, with a square padded Lift
#   down to the block.
#
# The variant (expr_attachment_changed) has Lift 8 mm.
#
# Mitcad expects: no fallback
# Mitcad change: Lift = 8 mm

sheet = spreadsheet("Spreadsheet", [("B1", "=5 mm", "Lift")])

stack = body("Stack")
base = block(stack, "StackBase", 30, 20, 10)
doc.recompute()

plane = stack.newObject("PartDesign::Plane", "LiftPlane")
attach(plane, [(origin_feature(stack, "XY_Plane"), "")], "FlatFace")
plane.AttachmentOffset = place((0, 0, 15))
bind(plane, ".AttachmentOffset.Base.z", "10 mm + Spreadsheet.Lift")
doc.recompute()
on_plane = stack.newObject("Sketcher::SketchObject", "OnPlaneSketch")
on_plane.Label = "OnPlaneSketch"
attach(on_plane, [(plane, "")])
add(on_plane, circle((8, 10), 4))
column = pad(stack, on_plane, 5, "Column", reversed_=True)
bind(column, "Length", "Spreadsheet.Lift")
doc.recompute()

on_face = sketch_on_face(stack, "OnFaceSketch", base, (0, 0, 1), 10)
on_face.AttachmentOffset = place((0, 0, 5))
bind(on_face, ".AttachmentOffset.Base.z", "Spreadsheet.Lift")
doc.recompute()
rectangle(on_face, 18, 5, 6, 6)
cube = pad(stack, on_face, 5, "Cube", reversed_=True)
bind(cube, "Length", "Spreadsheet.Lift")
doc.recompute()
stack.Tip = cube


def higher():
    sheet.set("B1", "=8 mm")


variant(higher)
