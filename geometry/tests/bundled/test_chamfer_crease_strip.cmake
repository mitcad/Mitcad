# SPDX-License-Identifier: MIT
# A chamfer across a slight concave crease of the tip's faces, over a side
# strip narrower than the chamfer whose foot face is a B-spline, with the
# port's patch 1000 (mitcad#343).
target_sources(test_geometry_chamfer PRIVATE tests/test_chamfer_crease_strip.cpp)
add_test(NAME geometry.chamfer_crease_strip COMMAND test_geometry_chamfer chamfer_crease_strip)
