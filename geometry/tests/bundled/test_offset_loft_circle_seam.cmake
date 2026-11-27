# SPDX-License-Identifier: MIT
# Closed inward offsets of lofts from a polygon to a circle whose seam
# splits a side, with the port's patch 1160 (mitcad#1199).
target_sources(test_geometry_loft_shell PRIVATE tests/test_offset_loft_circle_seam.cpp)
add_test(NAME geometry.offset_loft_circle_seam COMMAND test_geometry_loft_shell offset_loft_circle_seam)
set_tests_properties(geometry.offset_loft_circle_seam PROPERTIES TIMEOUT 300)
# The lean-in ones, the circle inside the polygon (mitcad#1199).
add_test(NAME geometry.offset_loft_circle_seam_lean_in COMMAND test_geometry_loft_shell offset_loft_circle_seam lean-in)
set_tests_properties(geometry.offset_loft_circle_seam_lean_in PROPERTIES TIMEOUT 600)
