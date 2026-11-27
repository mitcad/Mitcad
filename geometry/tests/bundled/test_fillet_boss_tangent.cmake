# SPDX-License-Identifier: MIT
# Roundings that end where a boss's cylinder meets a plate's flat side
# tangentially, on synthetic bodies and on a case of MITCAD_GEOMETRY_CASES,
# with the port's patches 1330 to 1332 (mitcad#1359); the plate's top edge
# and the boss's foot rounded together, with 1400 to 1402 (mitcad#1374).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_boss_tangent.cpp)
add_test(NAME geometry.fillet_boss_tangent COMMAND test_geometry_fillet_a_e fillet_boss_tangent)
set_tests_properties(geometry.fillet_boss_tangent PROPERTIES TIMEOUT 600)
