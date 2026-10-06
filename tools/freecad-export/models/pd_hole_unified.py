# SPDX-License-Identifier: MIT
# Holes with cosmetic threads of the Unified series, one per Body, each in
# a 30 x 20 x 15 block from a sketch on its top face:
#
# - Coarse: a 1/4 UNC hole, 10 mm deep, flat bottomed.
# - Fine: a 5/16 UNF hole through all, class 3B.
# - ExtraFine: a 1/2 UNEF hole, 12 mm deep, left-handed.
#
# Mitcad expects: no fallback


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


threaded(body("Coarse"), "CoarseHole", 0, "UNC", "1/4", Depth=10, DepthType="Dimension",
         DrillPoint="Flat")
threaded(body("Fine"), "FineHole", 40, "UNF", "5/16", DepthType="ThroughAll", ThreadClass="3B")
threaded(body("ExtraFine"), "ExtraFineHole", 80, "UNEF", "1/2", Depth=12, DepthType="Dimension",
         DrillPoint="Flat", ThreadDirection="Left")
