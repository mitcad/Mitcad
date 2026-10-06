# SPDX-License-Identifier: MIT
# Link arrays: a Part::Box (hidden) linked as a four-element array whose
# elements are objects of their own (ShowElement, App::LinkElement), each
# moved and turned, the third hidden (VisibilityList); and an App::Part with
# a body linked as a two-element array without element objects, placed by
# its PlacementList and placed itself; a third array of the box turned on
# top of the box's own placement (LinkTransform).

pin = named(doc.addObject("Part::Box", "Pin"), "Pin")
pin.Length, pin.Width, pin.Height = 4, 4, 12
pin.Placement = place((1, 1, 0))
hide(pin)
row = doc.addObject("App::Link", "Row")
row.Label = "Row"
row.setLink(pin)
row.ElementCount = 4
doc.recompute()
for i, element in enumerate(row.ElementList):
    element.Placement = place((10 * i, 0, 0), (0, 0, 1), 10 * i)
row.setElementVisible("2", False)

cell = part("Cell", place((0, 30, 0)))
block_body("CellBlock", 6, 6, 3, parent=cell)
pair = doc.addObject("App::Link", "Pair")
pair.Label = "Pair"
pair.setLink(cell)
pair.ShowElement = False
pair.ElementCount = 2
pair.PlacementList = [place((0, 0, 20 * i), (1, 0, 0), 90 * i) for i in range(2)]
pair.Placement = place((50, 0, 0))

column = doc.addObject("App::Link", "Column")
column.Label = "Column"
column.setLink(pin)
column.LinkTransform = True
column.ShowElement = False
column.ElementCount = 3
column.PlacementList = [place((0, 0, 15 * i), (1, 0, 0), 5 * i) for i in range(3)]
column.Placement = place((0, -30, 0), (0, 0, 1), 90)
