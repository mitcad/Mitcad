# SPDX-License-Identifier: MIT
# Holes with modelled threads, one per Body, each in a 30 x 20 x 15 block
# from a sketch on its top face. Mitcad cuts its basic profile from
# FreeCAD's bore, which is not FreeCAD's thread, so these take FreeCAD's
# stored shapes (fallbacks; the report gives the difference):
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
