# SPDX-License-Identifier: MIT
# Two stacked profiles with a step, the vertical edges that bound the
# upper and the lower face rounded as one feature: at the exact size, no
# warning, equal to the same edges as two features (mitcad#2165).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_stacked_step.cpp)
add_test(NAME geometry.fillet_stacked_step COMMAND test_geometry_fillet_o_s fillet_stacked_step)
set_tests_properties(geometry.fillet_stacked_step PROPERTIES TIMEOUT 300)
