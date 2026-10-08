# SPDX-License-Identifier: MIT
# Holes with cosmetic tyre valve threads (ISO 4570, FreeCAD 1.1 and
# later), one per Body, each in a 30 x 20 x 15 block from a sketch on its
# top face (Mitcad: a hole and a cosmetic thread of its tyre valve
# standard):
#
# - Core: a 5v1 hole, 10 mm deep, flat bottomed.
# - Cap: an 8v1 hole through all.
# - Large: a 12v1 hole, 12 mm deep, left-handed.
#
# Mitcad expects: no fallback

if not at_least(1, 1):
    raise Unsupported("tyre valve threads are new in FreeCAD 1.1")


def threaded(container, name, x, size, **options):
    base = block(container, name + "Base", 30, 20, 15, x, 0)
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), 15)
    add(sketch, circle((x + 15, 10), 3))
    feature = container.newObject("PartDesign::Hole", name)
    feature.Profile = sketch
    doc.recompute()
    feature.Threaded = True
    feature.ThreadType = "ISOTyre"
    feature.ThreadSize = size
    feature.ModelThread = False
    for key, value in options.items():
        setattr(feature, key, value)
    sketch.Visibility = False
    return feature


threaded(body("Core"), "CoreHole", 0, "5v1", Depth=10, DepthType="Dimension", DrillPoint="Flat")
threaded(body("Cap"), "CapHole", 40, "8v1", DepthType="ThroughAll")
threaded(body("Large"), "LargeHole", 80, "12v1", Depth=12, DepthType="Dimension",
         ThreadDirection="Left")
