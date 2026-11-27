# SPDX-License-Identifier: MIT
# A fillet whose edges lie in groups the geometry kernel rounds apart (a
# plate's three ribs, one 2 wide whose top a full round takes away), built
# group by group where they do not build at their exact size together
# (mitcad#2161): one valid solid, the wide ribs' material exact, the whole
# as the ribs rounded each alone.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_independent_groups.cpp)
target_link_libraries(test_geometry_fillet_f_n PRIVATE mitcad_geometry_analysis)
add_test(NAME geometry.fillet_independent_groups COMMAND test_geometry_fillet_f_n fillet_independent_groups)
set_tests_properties(geometry.fillet_independent_groups PROPERTIES TIMEOUT 300)
