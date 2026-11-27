# SPDX-License-Identifier: MIT
# A rounding whose contact runs off a narrow face onto an earlier blend of
# its radius rolls on that face's far edges, on cases of
# MITCAD_GEOMETRY_CASES, with the port's patch 1230 (mitcad#1215).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_roll_over_blend.cpp)
add_test(NAME geometry.fillet_roll_over_blend COMMAND test_geometry_fillet_o_s fillet_roll_over_blend)
set_tests_properties(geometry.fillet_roll_over_blend PROPERTIES TIMEOUT 600)
