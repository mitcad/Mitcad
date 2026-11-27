# SPDX-License-Identifier: MIT
# A convex edge rounded up into the mitre of two earlier convex roundings
# tighter than the ball, closed by a torus, on a synthetic block and a
# case of MITCAD_GEOMETRY_CASES, with the port's patch 1500 (mitcad#1414).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_convex_mitre.cpp)
add_test(NAME geometry.fillet_convex_mitre COMMAND test_geometry_fillet_a_e fillet_convex_mitre)
set_tests_properties(geometry.fillet_convex_mitre PROPERTIES TIMEOUT 600)
