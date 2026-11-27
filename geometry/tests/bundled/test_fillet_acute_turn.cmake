# SPDX-License-Identifier: MIT
# A concave rounding along an acute corner whose edge turns towards the
# ball, walked along the balls' centres, with the port's patch 1440
# (mitcad#1396); a case of MITCAD_GEOMETRY_CASES too.
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_acute_turn.cpp)
add_test(NAME geometry.fillet_acute_turn COMMAND test_geometry_fillet_a_e fillet_acute_turn)
set_tests_properties(geometry.fillet_acute_turn PROPERTIES TIMEOUT 600)
