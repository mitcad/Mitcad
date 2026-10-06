# SPDX-License-Identifier: MIT
# Pockets, one option per Body, each in a 30 x 20 x 10 block:
#
# - Deep: a circle on the top face, 4 mm deep.
# - Through: a circle on the top face, through all.
# - FromBelow: a circle on the XY plane (the bottom), reversed: 4 mm up.
# - Slot: a rectangle on the XZ plane across the block (which straddles
#   it), symmetric, 6 mm wide in all.
# - TwoSides: the same, 2 mm one way and 5 mm the other.
# - Both: a circle on the XZ plane, through all both ways.
#
# Mitcad expects: no fallback

deep = body("Deep")
base = block(deep, "DeepBase", 30, 20, 10)
sketch = sketch_on_face(deep, "DeepSketch", base, (0, 0, 1), 10)
add(sketch, circle((15, 10), 5))
pocket(deep, sketch, "DeepPocket", length=4)

through = body("Through")
base = block(through, "ThroughBase", 30, 20, 10, 40, 0)
sketch = sketch_on_face(through, "ThroughSketch", base, (0, 0, 1), 10)
add(sketch, circle((55, 10), 5))
pocket(through, sketch, "ThroughPocket", through_all=True)

below = body("FromBelow")
base = block(below, "FromBelowBase", 30, 20, 10, 80, 0)
sketch = sketch_on(below, "FromBelowSketch")
add(sketch, circle((95, 10), 5))
feature = pocket(below, sketch, "FromBelowPocket", length=4)
feature.Reversed = True

slot = body("Slot")
base = block(slot, "SlotBase", 30, 20, 10, 0, -50)
sketch = sketch_on(slot, "SlotSketch", plane="XZ_Plane", offset=place((0, 0, 40)))
rectangle(sketch, 10, 4, 10, 10)
feature = pocket(slot, sketch, "SlotPocket", length=6)
sides(feature, "symmetric")

two = body("TwoSides")
base = block(two, "TwoSidesBase", 30, 20, 10, 40, -50)
sketch = sketch_on(two, "TwoSidesSketch", plane="XZ_Plane", offset=place((0, 0, 40)))
rectangle(sketch, 50, 4, 10, 10)
feature = pocket(two, sketch, "TwoSidesPocket", length=2)
sides(feature, "two")
feature.Length2 = 5

both = body("Both")
base = block(both, "BothBase", 30, 20, 10, 80, -50)
sketch = sketch_on(both, "BothSketch", plane="XZ_Plane", offset=place((0, 0, 40)))
add(sketch, circle((95, 5), 3))
feature = pocket(both, sketch, "BothPocket", through_all=True)
sides(feature, "symmetric")
