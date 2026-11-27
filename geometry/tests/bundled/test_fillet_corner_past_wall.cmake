# SPDX-License-Identifier: MIT
# Two concave edges rounded at a convex corner left sharp, the roundings
# running past it into each other, with the port's patch 1060 (mitcad#840).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_corner_past_wall.cpp)
add_test(NAME geometry.fillet_corner_past_wall COMMAND test_geometry_fillet_a_e fillet_corner_past_wall)
set_tests_properties(geometry.fillet_corner_past_wall PROPERTIES TIMEOUT 60)
