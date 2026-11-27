# SPDX-License-Identifier: MIT
# A concave edge of a hole rounded wider than the straight stretch beside
# it, whose far edge goes on tangentially into the hole's round wall
# (GitHub #667, mitcad#2419): the ball takes the stretch away and rolls on
# over the wall, the space between the stretch and the wall continued
# filled first; strips 0.104 (the report's), 0.5 and 1.5 wide, the
# rounding 3 and the wall's radius 9, the volume exact.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_narrow_face_hole_wall.cpp)
add_test(NAME geometry.fillet_narrow_face_hole_wall COMMAND test_geometry_fillet_f_n fillet_narrow_face_hole_wall)
set_tests_properties(geometry.fillet_narrow_face_hole_wall PROPERTIES TIMEOUT 300)
