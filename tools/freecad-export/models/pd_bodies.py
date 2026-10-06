# SPDX-License-Identifier: MIT
# Several PartDesign Bodies: one at the origin, one placed (moved and turned
# about an oblique axis), and one whose Tip is an earlier feature than its
# last (its shape is the Tip's: the later pocket is rolled back), hidden.

block_body("Origin", 20, 10, 5)
block_body("Placed", 15, 15, 8, placement=place((30, 20, 10), (1, 2, 3), 40))

rolled = body("Rolled", placement=place((-40, 0, 0)))
rolled_base = rectangle_sketch(rolled, 20, 20, "RolledSketch")
rolled_pad = pad(rolled, rolled_base, 12, "RolledPad")
rolled_hole = circle_sketch(rolled, 5, "RolledHoleSketch", 10, 10)
pocket(rolled, rolled_hole, "RolledPocket", through_all=True)
doc.recompute()
rolled.Tip = rolled_pad
hide(rolled)
