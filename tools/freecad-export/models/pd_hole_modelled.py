# SPDX-License-Identifier: MIT
# Holes with modelled threads, one per Body, each in a 30 x 20 x 15 block
# from a sketch on its top face. Mitcad replays FreeCAD's construction (the
# groove's section swept along a helix, then the hole), which comes within
# 1.3e-6 to 2e-6 of FreeCAD's stored shapes, just above the import's 1e-6
# check (FreeCAD sweeps the section by the Frenet frame along a helix
# approximated turn by turn, Mitcad's helix keeps the axis as its
# binormal), so these still take FreeCAD's stored shapes; 1.0 does not
# model the metric one (the hole is invalid):
#
# - Metric: an M6 hole, 12 mm deep, flat bottomed, its thread modelled.
# - Unified: a 1/4 UNC hole through all, its thread modelled.


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
    for key, value in options.items():
        setattr(feature, key, value)
    feature.ModelThread = True
    sketch.Visibility = False
    return feature


threaded(body("Metric"), "MetricHole", 0, "ISOMetricProfile", "M6x1.0" if at_least(1, 1) else "M6",
         Depth=12, DepthType="Dimension", DrillPoint="Flat")
threaded(body("Unified"), "UnifiedHole", 40, "UNC", "1/4", DepthType="ThroughAll")
