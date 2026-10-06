# SPDX-License-Identifier: MIT
# Named sketch constraints and expressions between sketches and features:
#
# - Block: a rectangle with a named length (60 mm, no expression), its
#   height bound to .Constraints.length / 3 (the sketch's own), its corner
#   at shift = -length / 6 from the origin (a negative horizontal
#   distance), padded SketchA's height / 2.
# - Wedge: a triangle whose hypotenuse rises at slope (30 degrees, named)
#   and whose base (an unnamed constraint) is the block's height, padded
#   twice the block's pad (an expression of another feature's value).
# - Pin: a circle of radius pin (4 mm, named), padded pin * 3.
#
# The variant (expr_constraints_changed) has length 90 mm.
#
# Mitcad expects: no fallback
# Mitcad change: length = 90 mm

block_body = body("Block")
sketch_a = sketch_on(block_body, "SketchA")
lines = rectangle(sketch_a, -10, 0, 60, 20)
constrain(sketch_a, "PointOnObject", lines[0], START, H_AXIS)
constrain(sketch_a, "Horizontal", lines[0])
constrain(sketch_a, "Horizontal", lines[2])
constrain(sketch_a, "Vertical", lines[1])
constrain(sketch_a, "Vertical", lines[3])
length = constrain(sketch_a, "DistanceX", lines[0], START, lines[0], END, 60, name="length")
constrain(sketch_a, "DistanceY", lines[1], START, lines[1], END, 20, name="height")
bind(sketch_a, ".Constraints.height", ".Constraints.length / 3")
constrain(sketch_a, "DistanceX", H_AXIS, START, lines[0], START, -10, name="shift")
bind(sketch_a, ".Constraints.shift", "-.Constraints.length / 6")
pad_a = pad(block_body, sketch_a, 10, "PadA")
bind(pad_a, "Length", "SketchA.Constraints.height / 2")
doc.recompute()

wedge_body = body("Wedge")
sketch_b = sketch_on(wedge_body, "SketchB")
sides = polyline(sketch_b, [(70, 0), (90, 20 * math.tan(math.radians(30))), (90, 0)], closed=True)
constrain(sketch_b, "PointOnObject", sides[0], START, H_AXIS)
constrain(sketch_b, "DistanceX", H_AXIS, START, sides[0], START, 70)
constrain(sketch_b, "Vertical", sides[1])
constrain(sketch_b, "Horizontal", sides[2])
constrain(sketch_b, "Angle", sides[0], math.radians(30), name="slope")
base = constrain(sketch_b, "DistanceX", sides[2], END, sides[2], START, 20)
bind(sketch_b, "Constraints[%d]" % base, "SketchA.Constraints.height")
pad_b = pad(wedge_body, sketch_b, 20, "PadB")
bind(pad_b, "Length", "PadA.Length * 2")
doc.recompute()

pin_body = body("Pin")
sketch_c = sketch_on(pin_body, "SketchC")
rim = add(sketch_c, circle((0, 40), 4))
constrain(sketch_c, "PointOnObject", rim, MID, V_AXIS)
constrain(sketch_c, "DistanceY", H_AXIS, START, rim, MID, 40)
constrain(sketch_c, "Radius", rim, 4, name="pin")
pad_c = pad(pin_body, sketch_c, 12, "PadC")
bind(pad_c, "Length", "SketchC.Constraints.pin * 3")
doc.recompute()


def longer():
    sketch_a.setDatum(length, App.Units.Quantity("90 mm"))


variant(longer)
