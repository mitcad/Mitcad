# SPDX-License-Identifier: MIT
# A concave rounding along a wall's foot on a rod past concave arcs of the
# wall tighter than the ball, on synthetic bodies and on cases of
# MITCAD_GEOMETRY_CASES, with the port's patches 1380 and 1381
# (mitcad#1371).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_three_face_corner.cpp)
add_test(NAME geometry.fillet_three_face_corner COMMAND test_geometry_fillet_t_z fillet_three_face_corner)
set_tests_properties(geometry.fillet_three_face_corner PROPERTIES TIMEOUT 600)
