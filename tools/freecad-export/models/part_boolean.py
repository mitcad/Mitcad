# SPDX-License-Identifier: MIT
# Part workbench booleans: a box cut by a cylinder (Part::Cut), and two
# boxes fused (Part::MultiFuse) then filleted (Part::Fillet). The operands
# are consumed (and hidden, as FreeCAD's commands hide them): the results
# Cut and Fillet are the two bodies.

block = named(doc.addObject("Part::Box", "Block"), "Block")
block.Length, block.Width, block.Height = 40, 30, 20
hole = named(doc.addObject("Part::Cylinder", "Hole"), "Hole")
hole.Radius, hole.Height = 8, 30
hole.Placement = place((20, 15, -5))
cut = named(doc.addObject("Part::Cut", "Cut"), "Cut")
cut.Base, cut.Tool = block, hole
hide(block)
hide(hole)

a = named(doc.addObject("Part::Box", "A"), "A")
a.Length, a.Width, a.Height = 20, 20, 20
a.Placement = place((60, 0, 0))
b = named(doc.addObject("Part::Box", "B"), "B")
b.Length, b.Width, b.Height = 20, 20, 20
b.Placement = place((70, 10, 10), (0, 0, 1), 20)
fuse = named(doc.addObject("Part::MultiFuse", "Fuse"), "Fuse")
fuse.Shapes = [a, b]
hide(a)
hide(b)
doc.recompute()
fillet = named(doc.addObject("Part::Fillet", "Fillet"), "Fillet")
fillet.Base = fuse
# The vertical edges of the first box, by position.
edges = []
for i, edge in enumerate(fuse.Shape.Edges):
    d = edge.Vertexes[-1].Point - edge.Vertexes[0].Point
    c = edge.CenterOfMass
    if abs(d.x) < 1e-6 and abs(d.y) < 1e-6 and c.x < 61 and c.y < 1:
        edges.append((i + 1, 2.0, 2.0))
fillet.Edges = edges
hide(fuse)
