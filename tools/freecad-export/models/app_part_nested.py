# SPDX-License-Identifier: MIT
# App::Part nesting with placements: a Part (turned about Z and moved) holds
# a Body and a nested Part (turned about X and moved) with a Part::Box and a
# placed Body; a folder (DocumentObjectGroup, no placement) at the root
# holds a cylinder; a hidden Part holds a sphere.

outer = part("Outer", place((100, 0, 0), (0, 0, 1), 90))
block_body("OuterBlock", 20, 10, 5, parent=outer)
inner = part("Inner", place((0, 0, 30), (1, 0, 0), 30), parent=outer)
inner_box = named(doc.addObject("Part::Box", "InnerBox"), "InnerBox")
inner_box.Length, inner_box.Width, inner_box.Height = 8, 6, 4
inner_box.Placement = place((1, 2, 3))
inner.addObject(inner_box)
block_body("InnerBlock", 5, 5, 5, placement=place((10, 0, 0), (0, 1, 0), 90), parent=inner)

folder = doc.addObject("App::DocumentObjectGroup", "Folder")
folder.Label = "Folder"
cylinder = named(doc.addObject("Part::Cylinder", "FolderCylinder"), "FolderCylinder")
cylinder.Radius, cylinder.Height = 3, 9
cylinder.Placement = place((0, 50, 0))
folder.addObject(cylinder)

hidden = part("Hidden", place((0, -50, 0)))
sphere = named(doc.addObject("Part::Sphere", "HiddenSphere"), "HiddenSphere")
sphere.Radius = 4
hidden.addObject(sphere)
hide(hidden)
