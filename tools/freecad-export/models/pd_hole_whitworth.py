# SPDX-License-Identifier: MIT
# Holes with cosmetic Whitworth and NPT threads (FreeCAD 1.1 and later),
# one per Body, each in a 30 x 20 x 15 block from a sketch on its top face:
#
# - Coarse: a 1/4 BSW hole, 10 mm deep, flat bottomed.
# - Fine: a 5/16 BSF hole through all, class Normal.
# - Pipe: a 1/4 BSP (G 1/4) hole, 12 mm deep, flat bottomed, left-handed.
# - Taper: a 1/4 NPT hole, 12 mm deep, flat bottomed.
#
# Mitcad expects: no fallback

if not at_least(1, 1):
    raise Unsupported("Whitworth and NPT threads are new in FreeCAD 1.1")


def threaded(container, name, x, series, size, **options):
    base = block(container, name + "Base", 30, 20, 15, x, 0)
    sketch = sketch_on_face(container, name + "Sketch", base, (0, 0, 1), 15)
    add(sketch, circle((x + 15, 10), 3))
    feature = container.newObject("PartDesign::Hole", name)
    feature.Profile = sketch
    doc.recompute()
    feature.Threaded = True
    feature.ThreadType = series
    feature.ThreadSize = size
    feature.ModelThread = False
    for key, value in options.items():
        setattr(feature, key, value)
    sketch.Visibility = False
    return feature


threaded(body("Coarse"), "CoarseHole", 0, "BSW", "1/4", Depth=10, DepthType="Dimension",
         DrillPoint="Flat")
threaded(body("Fine"), "FineHole", 40, "BSF", "5/16", DepthType="ThroughAll", ThreadClass="Normal")
threaded(body("Pipe"), "PipeHole", 80, "BSP", "1/4", Depth=12, DepthType="Dimension",
         DrillPoint="Flat", ThreadDirection="Left")
threaded(body("Taper"), "TaperHole", 120, "NPT", "1/4", Depth=12, DepthType="Dimension",
         DrillPoint="Flat")
