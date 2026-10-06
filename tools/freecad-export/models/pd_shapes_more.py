# SPDX-License-Identifier: MIT
# Thickness modes and pipes through several sections, one per Body:
#
# - ThicknessPipe: a 30 x 20 x 10 block hollowed out to a 2 mm wall, its
#   top face removed, in FreeCAD's Pipe mode.
# - ThicknessRectoVerso: the same outwards in RectoVerso mode, the corners
#   joined by intersection.
# - PipeSections: a circle of radius 3 swept along a line and an arc to a
#   circle of radius 1.5 at the arc's end (AdditivePipe, Multisection).
#
# Mitcad expects: no fallback


def hollowed(container, name, x, **options):
    base = block(container, name + "Base", 30, 20, 10, x, 0)
    doc.recompute()
    feature = container.newObject("PartDesign::Thickness", name)
    feature.Base = (base, [face_index(base.Shape, (0, 0, 1), 10)])
    feature.Value = 2
    for key, value in options.items():
        setattr(feature, key, value)
    return feature


hollowed(body("ThicknessPipe"), "ThicknessPipeShell", 0, Mode="Pipe")
hollowed(body("ThicknessRectoVerso"), "ThicknessRectoVersoShell", 40, Mode="RectoVerso",
         Join="Intersection", Reversed=False)

piped = body("PipeSections")
spine = sketch_on(piped, "PipeSectionsSpine", offset=place((0, 40, 0)))
straight = add(spine, line((0, 0), (20, 0)))
bend = add(spine, arc((20, 10), 10, -90, 0))
constrain(spine, "Coincident", straight, END, bend, START)
start = sketch_on(piped, "PipeSectionsStart", plane="YZ_Plane", offset=place((40, 0, 0)))
add(start, circle((0, 0), 3))
end = sketch_on(piped, "PipeSectionsEnd", plane="XZ_Plane", offset=place((30, 0, -50)))
add(end, circle((0, 0), 1.5))
feature = piped.newObject("PartDesign::AdditivePipe", "PipeSectionsPipe")
feature.Profile = start
feature.Spine = (spine, ["Edge1", "Edge2"])
feature.Sections = [end]
feature.Transformation = "Multisection"
for sketch in (spine, start, end):
    sketch.Visibility = False
