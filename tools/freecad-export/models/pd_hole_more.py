# SPDX-License-Identifier: MIT
# Counterdrilled and tapered holes, one per Body, each in a 30 x 20 x 15
# block from a sketch on its top face:
#
# - Counterdrill: diameter 6, 10 mm deep, flat bottomed, counterdrilled
#   10 x 3 with a 90 degree cone.
# - CounterdrillThrough: through all, the cone 120 degrees.
# - Tapered: diameter 6, 10 mm deep, flat, its wall at 85 degrees.
# - TaperedPoint: 80 degrees, a 118 degree drill point.
# - TaperedThrough: 95 degrees (widening), through all.
# - TaperedCounterbore: 85 degrees under a 10 x 3 counterbore.
# - TaperedCountersink: 85 degrees under a 10 mm, 90 degree countersink.
#
# Mitcad expects: no fallback


def hole(container, name, x, **options):
    base = block(container, name + "Base", 30, 20, 15, x, 0)
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), 15)
    add(sketch, circle((x + 10, 10), 2))
    feature = container.newObject("PartDesign::Hole", name)
    feature.Profile = sketch
    doc.recompute()
    if "Counterdrill" not in feature.getEnumerationsOfProperty("HoleCutType"):
        raise Unsupported("no counterdrilled holes")
    for key, value in options.items():
        setattr(feature, key, value)
    sketch.Visibility = False
    return feature


hole(body("Counterdrill"), "CounterdrillHole", 0, Diameter=6, Depth=10, DepthType="Dimension",
     DrillPoint="Flat", HoleCutType="Counterdrill", HoleCutDiameter=10, HoleCutDepth=3,
     HoleCutCountersinkAngle=90)
hole(body("CounterdrillThrough"), "CounterdrillThroughHole", 40, Diameter=6, DepthType="ThroughAll",
     HoleCutType="Counterdrill", HoleCutDiameter=10, HoleCutDepth=3, HoleCutCountersinkAngle=120)
hole(body("Tapered"), "TaperedHole", 80, Diameter=6, Depth=10, DepthType="Dimension",
     DrillPoint="Flat", Tapered=True, TaperedAngle=85)
hole(body("TaperedPoint"), "TaperedPointHole", 120, Diameter=6, Depth=10, DepthType="Dimension",
     DrillPoint="Angled", DrillPointAngle=118, Tapered=True, TaperedAngle=80)
hole(body("TaperedThrough"), "TaperedThroughHole", 160, Diameter=6, DepthType="ThroughAll",
     Tapered=True, TaperedAngle=95)
hole(body("TaperedCounterbore"), "TaperedCounterboreHole", 200, Diameter=6, Depth=10,
     DepthType="Dimension", DrillPoint="Flat", Tapered=True, TaperedAngle=85,
     HoleCutType="Counterbore", HoleCutDiameter=10, HoleCutDepth=3)
hole(body("TaperedCountersink"), "TaperedCountersinkHole", 240, Diameter=6, Depth=10,
     DepthType="Dimension", DrillPoint="Flat", Tapered=True, TaperedAngle=85,
     HoleCutType="Countersink", HoleCutDiameter=10, HoleCutCountersinkAngle=90)
