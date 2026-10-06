# SPDX-License-Identifier: MIT
# More of the history's values driven by a spreadsheet:
#
# - Sizes: Bore (=6 mm), Deep (=8 mm), Edge (=1.5 mm), Rise (=10 mm),
#   Sweep (=120 deg), Gap (=Bore / 2 + 1 mm).
# - Holed: a 40 x 30 x 15 block, every edge chamfered Edge, a flat hole
#   of diameter Bore, Deep deep, at the centre of the top face.
# - Peg: an additive cylinder of radius Rise / 2 and height Rise * 2 on a
#   40 x 40 x 5 plate (60 mm off), counterbored holes beside it (diameter
#   Gap, through all, counterbore 2 Gap x Edge).
# - Turned: a 30 x 20 x 10 block (120 mm off) with a rectangle grooved
#   into it Sweep about its sketch's V axis (half each way).
#
# The variant (expr_features_changed) has Bore 8 mm and Edge 2 mm.
#
# Mitcad expects: no fallback
# Mitcad change: Bore = 8 mm
# Mitcad change: Edge = 2 mm

sheet = spreadsheet(
    "Sizes",
    [
        ("B1", "=6 mm", "Bore"),
        ("B2", "=8 mm", "Deep"),
        ("B3", "=1.5 mm", "Edge"),
        ("B4", "=10 mm", "Rise"),
        ("B5", "=120 deg", "Sweep"),
        ("B6", "=Bore / 2 + 1 mm", "Gap"),
    ],
)

holed = body("Holed")
base = block(holed, "HoledBase", 40, 30, 15)
doc.recompute()
chamfer = holed.newObject("PartDesign::Chamfer", "Chamfered")
chamfer.Base = (base, ["Edge1"])
chamfer.UseAllEdges = True
chamfer.Size = 1.5
bind(chamfer, "Size", "Sizes.Edge")
doc.recompute()
top = sketch_on(holed, "HoleSketch", offset=place((0, 0, 15)))
add(top, circle((20, 15), 3))
hole = holed.newObject("PartDesign::Hole", "Hole")
hole.Profile = top
doc.recompute()
hole.Diameter = 6
hole.Depth = 8
hole.DepthType = "Dimension"
hole.DrillPoint = "Flat"
bind(hole, "Diameter", "Sizes.Bore")
bind(hole, "Depth", "Sizes.Deep")
top.Visibility = False
doc.recompute()

peg = body("Peg")
plate = block(peg, "PegPlate", 40, 40, 5, 60, 0)
doc.recompute()
cylinder = peg.newObject("PartDesign::AdditiveCylinder", "PegCylinder")
cylinder.Placement = place((80, 20, 5))
cylinder.Radius = 5
cylinder.Height = 20
bind(cylinder, "Radius", "Sizes.Rise / 2")
bind(cylinder, "Height", "Sizes.Rise * 2")
doc.recompute()
holes = sketch_on(peg, "PegHoles", offset=place((0, 0, 5)))
add(holes, circle((67, 7), 2))
add(holes, circle((93, 33), 2))
counterbore = peg.newObject("PartDesign::Hole", "PegHole")
counterbore.Profile = holes
doc.recompute()
counterbore.Diameter = 4
counterbore.DepthType = "ThroughAll"
counterbore.HoleCutType = "Counterbore"
counterbore.HoleCutDiameter = 8
counterbore.HoleCutDepth = 1.5
bind(counterbore, "Diameter", "Sizes.Gap")
bind(counterbore, "HoleCutDiameter", "Sizes.Gap * 2")
bind(counterbore, "HoleCutDepth", "Sizes.Edge")
holes.Visibility = False
doc.recompute()

turned = body("Turned")
block(turned, "TurnedBase", 30, 20, 10, 120, 0)
profile = sketch_on(turned, "GrooveSketch", plane="XZ_Plane")
rectangle(profile, 125, 6, 5, 4)
groove = turned.newObject("PartDesign::Groove", "TurnedGroove")
groove.Profile = profile
groove.ReferenceAxis = (profile, ["V_Axis"])
groove.Angle = 120
# Half each way: the block lies on one side of the sketch's plane.
groove.Midplane = True
bind(groove, "Angle", "Sizes.Sweep")
profile.Visibility = False
doc.recompute()


def wider():
    sheet.set("B1", "=8 mm")
    sheet.set("B3", "=2 mm")


variant(wider)
