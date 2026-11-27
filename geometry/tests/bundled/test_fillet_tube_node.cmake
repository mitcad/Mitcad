# SPDX-License-Identifier: MIT
# Roundings of the valleys where tubes meet, run on through nodes whose
# facets are narrower than the radius (mitcad#1205).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_tube_node.cpp)
add_test(NAME geometry.fillet_tube_node COMMAND test_geometry_fillet_t_z fillet_tube_node)
set_tests_properties(geometry.fillet_tube_node PROPERTIES TIMEOUT 300)
