# SPDX-License-Identifier: MIT
# Fillets and chamfers, one option per Body, each on a 30 x 20 x 10 block
# (the edges named by position):
#
# - Fillet: the four vertical edges, radius 2.
# - FilletFace: every edge of the top face (the face as the reference),
#   radius 1.5.
# - FilletAll: every edge (UseAllEdges), radius 1.
# - Equal: the four top edges chamfered 1.5.
# - TwoDistances: one top edge chamfered 2 and 1.
# - Flipped: the same flipped (FlipDirection).
# - Angle: one top edge, 2 mm at 30 degrees.
# - Chained: a fillet after a chamfer of another edge (the fillet's edges
#   named in the chamfer's result).
#
# Mitcad expects: no fallback


def vertical(middle, edge):
    d = edge.Vertexes[-1].Point - edge.Vertexes[0].Point
    return abs(d.x) < 1e-9 and abs(d.y) < 1e-9


def top(middle, edge):
    return abs(middle.z - 10) < 1e-9


def dressup(container, kind, name, base, edges):
    feature = container.newObject(kind, name)
    feature.Base = (base, edges)
    return feature


fillet = body("Fillet")
base = block(fillet, "FilletBase", 30, 20, 10)
doc.recompute()
dressup(fillet, "PartDesign::Fillet", "FilletEdges", base, edges_where(base.Shape, vertical)).Radius = 2

face = body("FilletFace")
base = block(face, "FilletFaceBase", 30, 20, 10, 40, 0)
doc.recompute()
dressup(face, "PartDesign::Fillet", "FilletTop", base, [face_index(base.Shape, (0, 0, 1), 10)]).Radius = 1.5

every = body("FilletAll")
base = block(every, "FilletAllBase", 30, 20, 10, 80, 0)
doc.recompute()
feature = dressup(every, "PartDesign::Fillet", "FilletEvery", base, edges_where(base.Shape, vertical)[:1])
feature.UseAllEdges = True
feature.Radius = 1

equal = body("Equal")
base = block(equal, "EqualBase", 30, 20, 10, 0, 40)
doc.recompute()
dressup(equal, "PartDesign::Chamfer", "EqualChamfer", base, edges_where(base.Shape, top)).Size = 1.5

two = body("TwoDistances")
base = block(two, "TwoDistancesBase", 30, 20, 10, 40, 40)
doc.recompute()
feature = dressup(two, "PartDesign::Chamfer", "TwoDistancesChamfer", base,
                  edges_between(base.Shape, (40, 40, 10), (70, 40, 10)))
feature.ChamferType = "Two distances"
feature.Size = 2
feature.Size2 = 1

flipped = body("Flipped")
base = block(flipped, "FlippedBase", 30, 20, 10, 80, 40)
doc.recompute()
feature = dressup(flipped, "PartDesign::Chamfer", "FlippedChamfer", base,
                  edges_between(base.Shape, (80, 40, 10), (110, 40, 10)))
feature.ChamferType = "Two distances"
feature.Size = 2
feature.Size2 = 1
feature.FlipDirection = True

angle = body("Angle")
base = block(angle, "AngleBase", 30, 20, 10, 120, 40)
doc.recompute()
feature = dressup(angle, "PartDesign::Chamfer", "AngleChamfer", base,
                  edges_between(base.Shape, (120, 40, 10), (150, 40, 10)))
feature.ChamferType = "Distance and Angle"
feature.Size = 2
feature.Angle = 30

chained = body("Chained")
base = block(chained, "ChainedBase", 30, 20, 10, 0, 80)
doc.recompute()
chamfer = dressup(chained, "PartDesign::Chamfer", "ChainedChamfer", base,
                  edges_between(base.Shape, (0, 80, 10), (30, 80, 10)))
chamfer.Size = 2
doc.recompute()
dressup(chained, "PartDesign::Fillet", "ChainedFillet", chamfer,
        edges_where(chamfer.Shape, lambda m, e: vertical(m, e) and m.y > 90)).Radius = 1.5
