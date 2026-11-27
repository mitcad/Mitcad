# SPDX-License-Identifier: MIT
# A convex and a concave edge chamfered at a vertex where a sharp edge goes
# on, the corner the same at both ends of the bracket, with the port's
# patch 1090 (mitcad#1005).
target_sources(test_geometry_chamfer PRIVATE tests/test_chamfer_sharp_corner.cpp)
target_link_libraries(test_geometry_chamfer PRIVATE mitcad_geometry_io)
add_test(NAME geometry.chamfer_sharp_corner COMMAND test_geometry_chamfer chamfer_sharp_corner)
set_tests_properties(geometry.chamfer_sharp_corner PROPERTIES TIMEOUT 60)
