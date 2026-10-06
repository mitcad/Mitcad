# SPDX-License-Identifier: MIT
# Part workbench operations on sketches and primitives (the operands hidden,
# as FreeCAD's commands hide them):
#
# - Extruded: a 20 x 10 rectangle sketch extruded 10 mm along its normal.
# - Symmetric: the same, symmetric about the sketch.
# - TwoWays: 8 mm forward and 3 mm back.
# - Reversed: 10 mm the other way.
# - Tapered: tapered 5 degrees.
# - Turned: a rectangle turned 360 degrees about the Y axis.
# - Quarter: the same, 90 degrees.
# - Common: the common of a box and a sphere (Part::MultiCommon).
# - Chamfered: a box with two edges chamfered 1.5 (Part::Chamfer).
# - Mirrored: a cone mirrored in the plane x = 150 (Part::Mirroring).
# - Shared: one cylinder cut out of two boxes (two Part::Cut, one tool).
#
# Mitcad expects: no fallback


def rectangle_profile(name, x, y, width, height, at=None):
    sketch = doc.addObject("Sketcher::SketchObject", name)
    sketch.Label = name
    if at is not None:
        sketch.Placement = at
    rectangle(sketch, x, y, width, height)
    return sketch


def extrusion(name, profile, **options):
    feature = named(doc.addObject("Part::Extrusion", name), name)
    feature.Base = profile
    feature.DirMode = "Normal"
    feature.Solid = True
    feature.LengthFwd = 10
    for key, value in options.items():
        setattr(feature, key, value)
    hide(profile)
    return feature


extrusion("Extruded", rectangle_profile("ExtrudedSketch", 0, 0, 20, 10))
extrusion("Symmetric", rectangle_profile("SymmetricSketch", 30, 0, 20, 10), Symmetric=True)
extrusion("TwoWays", rectangle_profile("TwoWaysSketch", 60, 0, 20, 10), LengthFwd=8, LengthRev=3)
extrusion("Reversed", rectangle_profile("ReversedSketch", 90, 0, 20, 10), Reversed=True)
extrusion("Tapered", rectangle_profile("TaperedSketch", 120, 0, 20, 10), TaperAngle=5)


def revolution(name, profile, angle):
    feature = named(doc.addObject("Part::Revolution", name), name)
    feature.Source = profile
    feature.Axis = V(0, 1, 0)
    feature.Base = V(0, 0, 0)
    feature.Angle = angle
    feature.Solid = True
    hide(profile)
    return feature


revolution("Turned", rectangle_profile("TurnedSketch", 10, 30, 5, 10), 360)
revolution("Quarter", rectangle_profile("QuarterSketch", 10, 30, 5, 10, place((0, 0, 40))), 90)

box = named(doc.addObject("Part::Box", "CommonBox"), "CommonBox")
box.Length, box.Width, box.Height = 20, 20, 20
box.Placement = place((0, 80, 0))
ball = named(doc.addObject("Part::Sphere", "CommonBall"), "CommonBall")
ball.Radius = 13
ball.Placement = place((10, 90, 10))
common = named(doc.addObject("Part::MultiCommon", "Common"), "Common")
common.Shapes = [box, ball]
hide(box)
hide(ball)

plain = named(doc.addObject("Part::Box", "ChamferBox"), "ChamferBox")
plain.Length, plain.Width, plain.Height = 20, 15, 10
plain.Placement = place((40, 80, 0))
doc.recompute()
chamfer = named(doc.addObject("Part::Chamfer", "Chamfered"), "Chamfered")
chamfer.Base = plain
chamfer.Edges = [(int(n[4:]), 1.5, 1.5) for n in
                 edges_where(plain.Shape, lambda m, e: abs(m.z - 10) < 1e-9 and m.y < 81)
                 + edges_where(plain.Shape, lambda m, e: abs(m.z - 10) < 1e-9 and m.x > 59)]
hide(plain)

cone = named(doc.addObject("Part::Cone", "MirrorCone"), "MirrorCone")
cone.Radius1, cone.Radius2, cone.Height = 6, 2, 10
cone.Placement = place((130, 90, 0))
mirrored = named(doc.addObject("Part::Mirroring", "Mirrored"), "Mirrored")
mirrored.Source = cone
mirrored.Normal = V(1, 0, 0)
mirrored.Base = V(150, 0, 0)
hide(cone)

tool = named(doc.addObject("Part::Cylinder", "SharedTool"), "SharedTool")
tool.Radius, tool.Height = 4, 40
tool.Placement = place((0, 135, -5))
for i, x in enumerate((-10, 0)):
    target = named(doc.addObject("Part::Box", "SharedBox%d" % i), "SharedBox%d" % i)
    target.Length, target.Width, target.Height = 10, 10, 10
    target.Placement = place((x, 130, 15 * i))
    cut = named(doc.addObject("Part::Cut", "Shared%d" % i), "Shared%d" % i)
    cut.Base, cut.Tool = target, tool
    hide(target)
hide(tool)
