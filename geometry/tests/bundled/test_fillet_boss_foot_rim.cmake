# SPDX-License-Identifier: MIT
# A boss's foot and rim rounded together with half the boss's height, the
# foot's contact along part of the rim's, on synthetic bodies, with the
# port's patch 1340 (mitcad#1392).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_boss_foot_rim.cpp)
add_test(NAME geometry.fillet_boss_foot_rim COMMAND test_geometry_fillet_a_e fillet_boss_foot_rim)
set_tests_properties(geometry.fillet_boss_foot_rim PROPERTIES TIMEOUT 600)
