# SPDX-License-Identifier: MIT
# Links to another file: a second document (link_external_part.FCStd, saved
# next to this one) holds a Body and an App::Part with a box; this document
# links the Body twice and the Part once (App::Link with an XLink to the
# other file, by a relative path), and has a body of its own.

other = App.newDocument("link_external_part")
other_body = other.addObject("PartDesign::Body", "Bolt")
other_body.Label = "Bolt"
other_body.Placement = place((0, 0, 5))
shank = circle_sketch(other_body, 3, "ShankSketch")
pad(other_body, shank, 20, "ShankPad")
head = circle_sketch(other_body, 5, "HeadSketch", offset=place((0, 0, 20)))
pad(other_body, head, 4, "HeadPad")
other_part = other.addObject("App::Part", "Kit")
other_part.Label = "Kit"
other_part.Placement = place((0, 0, 0), (0, 0, 1), 30)
other_box = other.addObject("Part::Box", "KitBox")
other_box.Label = "KitBox"
other_part.addObject(other_box)
save_extra(other, "link_external_part")

base = block_body("Base", 60, 20, 5)
save_model()
for i, x in enumerate((10, 50)):
    link("Bolt%d" % (i + 1), other_body, place((x, 10, 5)))
link("KitCopy", other_part, place((0, 40, 0)))
