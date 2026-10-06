# SPDX-License-Identifier: MIT
# App::Link: a Body (placed, hidden as the original of its links) linked
# twice, once replacing its placement (LinkTransform false, the default)
# and once on top of it (LinkTransform true); an App::Part with a box and a
# body linked once; a link of a link.

bracket = block_body("Bracket", 20, 10, 4, placement=place((0, 0, 0), (0, 0, 1), 15))
color(bracket, (0.7, 0.2, 0.7))
hide(bracket)
link("BracketA", bracket, place((50, 0, 0)))
link("BracketB", bracket, place((0, 50, 0), (1, 0, 0), 90), transform=True)

unit = part("Unit", place((0, 0, 40), (0, 0, 1), 45))
unit_box = named(doc.addObject("Part::Box", "UnitBox"), "UnitBox")
unit_box.Length, unit_box.Width, unit_box.Height = 6, 6, 6
unit.addObject(unit_box)
block_body("UnitBlock", 10, 4, 2, placement=place((10, 0, 0)), parent=unit)
link("UnitCopy", unit, place((-60, 0, 0), (0, 1, 0), 30))

first = link("Relay", bracket, place((0, -50, 0)))
link("RelayOfRelay", first, place((0, -80, 10), (0, 0, 1), 60))
