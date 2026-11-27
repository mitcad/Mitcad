# SPDX-License-Identifier: MIT
# A chamfer of a concave edge wider than the narrow face beside it, cut by
# the face beyond, never hanging out past it (mitcad#1260).
target_sources(test_geometry_chamfer PRIVATE tests/test_chamfer_overflow_concave.cpp)
target_link_libraries(test_geometry_chamfer PRIVATE mitcad_geometry_io mitcad_geometry_analysis)
add_test(NAME geometry.chamfer_overflow_concave COMMAND test_geometry_chamfer chamfer_overflow_concave)
set_tests_properties(geometry.chamfer_overflow_concave PROPERTIES TIMEOUT 300)
