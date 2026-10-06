# SPDX-License-Identifier: MIT
# FreeCAD's functions, constants, units and operators in spreadsheet
# cells (compared with FreeCAD's values of them in the dump), and
# expressions Mitcad keeps as values:
#
# - Functions: pi, e, mod, cbrt, trunc, log, log10, exp, average and sum
#   of ranges, min and max of three, hypot, cath, sqrt, abs, round,
#   floor, ceil, trig in degrees, asin, atan2, pow; -2^2 (FreeCAD's sign
#   binds tighter) and 2^3^2 (left to right); units in, ft, thou, cm, m,
#   rad, °; a conditional; a cell of another unit (kg) and a text.
# - Box: a pad of a rectangle, its length Size (=cath(5 mm; 3 mm) * 5),
#   tapered by Tilt (=asin(0.5) / 10); a pocket half the pad's bounding
#   box height deep (a shape's measure: kept as a value).
#
# Mitcad expects: no fallback

spreadsheet(
    "Functions",
    [
        ("A1", "=pi * 2", "TwoPi"),
        ("A2", "=e", "Euler"),
        ("A3", "=mod(7; 3)", "Modulo"),
        ("A4", "=cbrt(27)", "CubeRoot"),
        ("A5", "=cbrt(-8 mm^3)", "CubeRootLength"),
        ("A6", "=trunc(-2.5)", "Truncated"),
        ("A7", "=log10(100)", "Decimal"),
        ("A8", "=log(e ^ 2)", "Natural"),
        ("A9", "=exp(1)", "Exponent"),
        ("A10", "=average(A3:A4)", "Mean"),
        ("A11", "=sum(A3:A4; A7)", "Total"),
        ("A12", "=max(1 mm; 3 mm; 2 mm)", "Largest"),
        ("A13", "=min(4; 2; 3)", "Smallest"),
        ("A14", "=hypot(6 mm; 8 mm)", "Hypotenuse"),
        ("A15", "=sqrt(16 mm^2)", "Root"),
        ("A16", "=abs(-3 mm)", "Absolute"),
        ("A17", "=round(2.5)", "Rounded"),
        ("A18", "=floor(-2.5)", "Floor"),
        ("A19", "=ceil(2.1)", "Ceiling"),
        ("A20", "=sin(30 deg) + cos(60)", "Trig"),
        ("A21", "=tan(45 °)", "Tangent"),
        ("A22", "=atan2(1; 1)", "Direction"),
        ("A23", "=pow(2; 10)", "Power"),
        ("A24", "=-2 ^ 2", "SignFirst"),
        ("A25", "=2 ^ 3 ^ 2", "PowerChain"),
        ("A26", "=1 in + 2 ft", "Imperial"),
        ("A27", "=3 thou + 1 cm + 0.001 m", "Mixed"),
        ("A28", "=0.5 rad + 10 °", "Turned"),
        ("A29", "=A12 > 2 mm ? A14 : A15", "Chosen"),
        ("A30", "=10 kg", "Mass"),
        ("A31", "'text"),
        ("B1", "=cath(5 mm; 3 mm) * 5", "Size"),
        ("B2", "=asin(0.5) / 10", "Tilt"),
    ],
)

box = body("Box")
sketch = sketch_on(box, "BoxSketch")
rectangle(sketch, 0, 0, 30, 20)
block_pad = pad(box, sketch, 20, "BoxPad")
bind(block_pad, "Length", "Functions.Size")
block_pad.TaperAngle = 3
bind(block_pad, "TaperAngle", "Functions.Tilt")
doc.recompute()
top = sketch_on(box, "TopSketch", offset=place((0, 0, 20)))
add(top, circle((15, 10), 3))
hole = pocket(box, top, "TopPocket", length=10)
bind(hole, "Length", "BoxPad.Shape.BoundBox.ZLength / 2")
doc.recompute()
