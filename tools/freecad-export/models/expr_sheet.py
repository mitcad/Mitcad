# SPDX-License-Identifier: MIT
# A Body driven by a spreadsheet: aliases, a cell named by its address and
# formulas over other cells.
#
# - Spreadsheet: Width (=50 mm), Depth (=Width * 0.6), Height (=12 mm),
#   B4 (=Height / 2, no alias), Hole (=hypot(3 mm; 4 mm) / 2), Count (=3),
#   Gap (=Width > 55 mm ? 12 mm : 8 mm), Edge (=min(Depth; Width) / 10),
#   Total (=sum(B1:B3)); a text cell beside them.
# - Block: a rectangle whose named constraints width and depth are Width
#   and Depth (the second by the sheet's label), padded Height; its edges
#   filleted by Edge (Rounded, all edges).
# - Holes: a circle of radius Hole on a sketch Height above the XY plane
#   (an attachment offset bound to Height), Depth / 2 from the X axis,
#   pocketed B4 deep, Count of them over Gap * (Count - 1) along X.
#
# The variant (expr_sheet_changed) has Width 60 mm: deeper, and the holes
# Gap 12 mm apart.
#
# Mitcad expects: no fallback
# Mitcad change: Width = 60 mm

sheet = spreadsheet(
    "Spreadsheet",
    [
        ("A1", "'Width"),
        ("B1", "=50 mm", "Width"),
        ("B2", "=Width * 0.6", "Depth"),
        ("B3", "=12 mm", "Height"),
        ("B4", "=Height / 2"),
        ("B5", "=hypot(3 mm; 4 mm) / 2", "Hole"),
        ("B6", "=3", "Count"),
        ("B7", "=Width > 55 mm ? 12 mm : 8 mm", "Gap"),
        ("B8", "=min(Depth; Width) / 10", "Edge"),
        ("B9", "=sum(B1:B3)", "Total"),
    ],
)

block_body = body("Block")
sketch = sketch_on(block_body, "BlockSketch")
lines = rectangle(sketch, 0, 0, 50, 30)
constrain(sketch, "Coincident", lines[0], START, H_AXIS, START)
constrain(sketch, "Horizontal", lines[0])
constrain(sketch, "Horizontal", lines[2])
constrain(sketch, "Vertical", lines[1])
constrain(sketch, "Vertical", lines[3])
constrain(sketch, "DistanceX", lines[0], START, lines[0], END, 50, name="width")
constrain(sketch, "DistanceY", lines[1], START, lines[1], END, 30, name="depth")
bind(sketch, ".Constraints.width", "Spreadsheet.Width")
bind(sketch, ".Constraints.depth", "<<Spreadsheet>>.Depth")
block_pad = pad(block_body, sketch, 12, "BlockPad")
bind(block_pad, "Length", "Spreadsheet.Height")
doc.recompute()

# All the block's edges: FreeCAD 1.0 and 1.1 lose the names of chosen
# edges when the width changes (and fail the variant).
fillet = block_body.newObject("PartDesign::Fillet", "Rounded")
fillet.Base = (block_pad, ["Edge1"])
fillet.UseAllEdges = True
fillet.Radius = 3
bind(fillet, "Radius", "Spreadsheet.Edge")
doc.recompute()

holes = sketch_on(block_body, "HoleSketch", offset=place((0, 0, 12)))
bind(holes, ".AttachmentOffset.Base.z", "Spreadsheet.Height")
hole = add(holes, circle((10, 15), 2.5))
radius = constrain(holes, "Radius", hole, 2.5)
bind(holes, "Constraints[%d]" % radius, "Spreadsheet.Hole")
constrain(holes, "DistanceX", H_AXIS, START, hole, MID, 10, name="hole_x")
hole_y = constrain(holes, "DistanceY", H_AXIS, START, hole, MID, 15)
bind(holes, "Constraints[%d]" % hole_y, "Spreadsheet.Depth / 2")
hole_pocket = pocket(block_body, holes, "HolePocket", length=6)
bind(hole_pocket, "Length", "Spreadsheet.B4")
doc.recompute()

pattern = block_body.newObject("PartDesign::LinearPattern", "HolePattern")
pattern.Originals = [hole_pocket]
pattern.Direction = (origin_feature(block_body, "X_Axis"), [""])
pattern.Length = 16
pattern.Occurrences = 3
bind(pattern, "Occurrences", "Spreadsheet.Count")
bind(pattern, "Length", "Spreadsheet.Gap * (Spreadsheet.Count - 1)")
doc.recompute()
# A transformation made by a script does not become its Body's tip.
block_body.Tip = pattern


def wider():
    sheet.set("B1", "=60 mm")


variant(wider)
