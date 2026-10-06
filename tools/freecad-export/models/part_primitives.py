# SPDX-License-Identifier: MIT
# Part workbench primitives at placements of their own: a box turned about
# Z, a cylinder tilted about X, a cone and a torus moved, coloured; a
# sphere hidden. Each is a body of its own (nothing consumes them).

box = named(doc.addObject("Part::Box", "Box"), "Box")
box.Length, box.Width, box.Height = 30, 20, 10
box.Placement = place((5, -3, 2), (0, 0, 1), 30)
color(box, (0.9, 0.1, 0.1))

cylinder = named(doc.addObject("Part::Cylinder", "Cylinder"), "Cylinder")
cylinder.Radius, cylinder.Height = 6, 25
cylinder.Placement = place((40, 0, 0), (1, 0, 0), -60)
color(cylinder, (0.1, 0.6, 0.2))

sphere = named(doc.addObject("Part::Sphere", "Sphere"), "Sphere")
sphere.Radius = 7
sphere.Placement = place((0, 40, 7))
hide(sphere)

cone = named(doc.addObject("Part::Cone", "Cone"), "Cone")
cone.Radius1, cone.Radius2, cone.Height = 8, 3, 12
cone.Placement = place((-30, 10, 0), (0, 1, 0), 15)

torus = named(doc.addObject("Part::Torus", "Torus"), "Torus")
torus.Radius1, torus.Radius2 = 12, 3
torus.Placement = place((0, -40, 5), (1, 1, 0), 45)
color(torus, (0.2, 0.3, 0.9))
