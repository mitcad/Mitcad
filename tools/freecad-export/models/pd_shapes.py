# SPDX-License-Identifier: MIT
# Drafts, thickness, lofts and pipes, one per Body:
#
# - Draft: the four side faces of a 30 x 20 x 10 block drafted 5 degrees
#   about its bottom face.
# - Thickness: a block hollowed out to a 2 mm wall, its top face removed.
# - Loft: a 20 x 20 square on the XY plane lofted to a 10 x 10 square
#   20 mm up (AdditiveLoft).
# - Pipe: a circle of radius 2 swept along a line and an arc
#   (AdditivePipe).
#
# Mitcad expects: no fallback

drafted = body("Draft")
base = block(drafted, "DraftBase", 30, 20, 10)
doc.recompute()
faces = []
for i, face in enumerate(base.Shape.Faces):
    if abs(face.normalAt(0, 0).z) < 1e-9:
        faces.append("Face%d" % (i + 1))
feature = drafted.newObject("PartDesign::Draft", "Drafted")
feature.Base = (base, faces)
feature.NeutralPlane = (base, [face_index(base.Shape, (0, 0, -1), 0)])
feature.Angle = 5

hollow = body("Thickness")
base = block(hollow, "ThicknessBase", 30, 20, 10, 40, 0)
doc.recompute()
feature = hollow.newObject("PartDesign::Thickness", "Hollowed")
feature.Base = (base, [face_index(base.Shape, (0, 0, 1), 10)])
feature.Value = 2

lofted = body("Loft")
bottom = sketch_on(lofted, "LoftBottom")
rectangle(bottom, 80, 0, 20, 20)
upper = sketch_on(lofted, "LoftTop", offset=place((0, 0, 20)))
rectangle(upper, 85, 5, 10, 10)
feature = lofted.newObject("PartDesign::AdditiveLoft", "Lofted")
feature.Profile = bottom
feature.Sections = [upper]
bottom.Visibility = False
upper.Visibility = False

piped = body("Pipe")
spine = sketch_on(piped, "PipeSpine", offset=place((0, 40, 0)))
straight = add(spine, line((0, 0), (20, 0)))
bend = add(spine, arc((20, 10), 10, -90, 0))
constrain(spine, "Coincident", straight, END, bend, START)
profile = sketch_on(piped, "PipeProfile", plane="YZ_Plane", offset=place((40, 0, 0)))
add(profile, circle((0, 0), 2))
feature = piped.newObject("PartDesign::AdditivePipe", "Piped")
feature.Profile = profile
feature.Spine = (spine, ["Edge1", "Edge2"])
spine.Visibility = False
profile.Visibility = False
