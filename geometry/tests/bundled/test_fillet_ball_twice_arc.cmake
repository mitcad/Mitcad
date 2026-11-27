# SPDX-License-Identifier: MIT
# A pocket's floor rounded with twice the radius of its concave corner
# arcs, on a synthetic body and on a case of MITCAD_GEOMETRY_CASES, with
# the port's patch 1390 (mitcad#1368).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_ball_twice_arc.cpp)
add_test(NAME geometry.fillet_ball_twice_arc COMMAND test_geometry_fillet_a_e fillet_ball_twice_arc)
set_tests_properties(geometry.fillet_ball_twice_arc PROPERTIES TIMEOUT 600)
