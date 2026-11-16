# SPDX-License-Identifier: MIT
# The sketch snapper's body vertices and edge midpoints (mitcad#258): the
# sketch model's data with OCCT's geometry types.
target_sources(test_app PRIVATE tests/sketch_snap.cpp sketch/SketchSnap.cpp sketch/BodyGeometry.cpp)
