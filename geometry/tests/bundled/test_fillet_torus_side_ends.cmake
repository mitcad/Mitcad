# SPDX-License-Identifier: MIT
# A wall's top and bottom edges rounded past a concave corner of the wall
# into a tip whose edges are rounded tighter than the ball: the blend run
# on past the chain's end over both faces' surfaces and cut by the faces
# it runs into, on a case of MITCAD_GEOMETRY_CASES, with the port's
# patches 1580 and 1581 (mitcad#1425).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_torus_side_ends.cpp)
add_test(NAME geometry.fillet_torus_side_ends COMMAND test_geometry_fillet_t_z fillet_torus_side_ends)
set_tests_properties(geometry.fillet_torus_side_ends PROPERTIES TIMEOUT 600)
