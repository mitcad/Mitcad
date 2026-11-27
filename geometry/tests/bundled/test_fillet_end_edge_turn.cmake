# SPDX-License-Identifier: MIT
# A rounding whose contact leaves its face across a convex edge into the
# face at its end, short of the end: the ball rolled on that edge and the
# face at the end cutting it from where its contact turns back, on a case
# of MITCAD_GEOMETRY_CASES, with the port's patch 1430 (mitcad#1389).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_end_edge_turn.cpp)
add_test(NAME geometry.fillet_end_edge_turn COMMAND test_geometry_fillet_a_e fillet_end_edge_turn)
set_tests_properties(geometry.fillet_end_edge_turn PROPERTIES TIMEOUT 600)
