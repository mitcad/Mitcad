# SPDX-License-Identifier: MIT
# Holes, one option per Body, each in a 30 x 20 x 15 block from a sketch on
# its top face (holes at the sketch's circles' centres):
#
# - Simple: diameter 6, 8 mm deep, an angled (118 degree) drill point.
# - Flat: diameter 5, 6 mm deep, flat bottomed; two circles, two holes.
# - Through: diameter 4, through all.
# - Counterbore: diameter 6 through all, counterbored 10 x 3.
# - Countersink: diameter 6 through all, countersunk 12 at 90 degrees.
# - Threaded: an M6 hole, 10 mm deep, its thread not modelled.
#
# Mitcad expects: no fallback


def hole(container, name, x, circles=1, **options):
    base = block(container, name + "Base", 30, 20, 15, x, 0)
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), 15)
    for i in range(circles):
        add(sketch, circle((x + 10 + 10 * i, 10), 2))
    feature = container.newObject("PartDesign::Hole", name)
    feature.Profile = sketch
    # The hole's options read the sketch's shape.
    doc.recompute()
    for key, value in options.items():
        setattr(feature, key, value)
    sketch.Visibility = False
    return feature


hole(body("Simple"), "SimpleHole", 0, Diameter=6, Depth=8, DepthType="Dimension",
     DrillPoint="Angled", DrillPointAngle=118)
hole(body("Flat"), "FlatHole", 40, circles=2, Diameter=5, Depth=6, DepthType="Dimension",
     DrillPoint="Flat")
hole(body("Through"), "ThroughHole", 80, Diameter=4, DepthType="ThroughAll")
hole(body("Counterbore"), "CounterboreHole", 120, Diameter=6, DepthType="ThroughAll",
     HoleCutType="Counterbore", HoleCutDiameter=10, HoleCutDepth=3)
hole(body("Countersink"), "CountersinkHole", 160, Diameter=6, DepthType="ThroughAll",
     HoleCutType="Countersink", HoleCutDiameter=12, HoleCutCountersinkAngle=90)
threaded = hole(body("Threaded"), "ThreadedHole", 200, Depth=10, DepthType="Dimension",
                DrillPoint="Flat")
threaded.Threaded = True
threaded.ThreadType = "ISOMetricProfile"
# The sizes' names differ between versions ("M6", "M6x1.0").
sizes = threaded.getEnumerationsOfProperty("ThreadSize")
threaded.ThreadSize = [s for s in sizes if s == "M6" or s.startswith("M6x")][0]
threaded.ModelThread = False
