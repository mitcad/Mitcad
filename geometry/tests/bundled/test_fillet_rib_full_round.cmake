# SPDX-License-Identifier: MIT
# A rib's four top edges rounded or bevelled by half its width (mitcad#1500,
# the port's patch 1655): the long edges' contacts meet on one ridge edge,
# the top face gone, no face of no area, the exact shape's volume. And a
# cube rounded on all its edges by 4.9998 (mitcad#1922, patch 1701): the
# join's contacts of no length no longer hang it, and it is never given
# back unrounded as done.
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_rib_full_round.cpp)
add_test(NAME geometry.fillet_rib_full_round COMMAND test_geometry_fillet_o_s fillet_rib_full_round)
set_tests_properties(geometry.fillet_rib_full_round PROPERTIES TIMEOUT 600)
