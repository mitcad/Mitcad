# SPDX-License-Identifier: MIT
# A rib's sloped edge rounded 1 degree from a bent guide's end, with
# OCCT's fillet alone, on a case of MITCAD_GEOMETRY_CASES: the corner's
# set-back walks over the rim's end vertex (port patch 1801), a walk
# starting on its face's edge ends there (1802) (mitcad#2018).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_corner_walk_over_vertex.cpp)
add_test(NAME geometry.fillet_corner_walk_over_vertex COMMAND test_geometry_fillet_a_e fillet_corner_walk_over_vertex)
