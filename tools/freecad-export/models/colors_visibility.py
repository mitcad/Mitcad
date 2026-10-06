# SPDX-License-Identifier: MIT
# Colours and visibility: bodies in several colours, one in FreeCAD's
# default colour, one with a colour per face, one hidden Body and one hidden
# App::Part (its content hidden with it).

red = named(doc.addObject("Part::Box", "Red"), "Red")
color(red, (1.0, 0.0, 0.0))
green = block_body("Green", 10, 10, 10, placement=place((20, 0, 0)))
color(green, (0.0, 0.8, 0.0))
plain = named(doc.addObject("Part::Box", "Plain"), "Plain")
plain.Placement = place((40, 0, 0))
faces = named(doc.addObject("Part::Box", "Faces"), "Faces")
faces.Placement = place((60, 0, 0))
doc.recompute()
face_colors(
    faces,
    [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (1.0, 1.0, 0.0), (0.0, 1.0, 1.0), (1.0, 0.0, 1.0)],
)
ghost = block_body("Ghost", 5, 5, 5, placement=place((0, 20, 0)))
color(ghost, (0.5, 0.5, 1.0))
hide(ghost)
shelf = part("Shelf", place((0, 40, 0)))
shelf_box = named(doc.addObject("Part::Box", "ShelfBox"), "ShelfBox")
color(shelf_box, (0.3, 0.3, 0.3))
shelf.addObject(shelf_box)
hide(shelf)
