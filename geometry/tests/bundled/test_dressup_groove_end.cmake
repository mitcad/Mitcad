# SPDX-License-Identifier: MIT
# The end of a shallow swept groove (a circle of radius 10, 1.5 deep,
# along a line or an arc) rounded wider than its end cap is high
# (mitcad#2154): below the circle's radius a torus piece, at it a sphere
# piece, the end cap gone and the volume the region of revolution's;
# above it a failure in words.
target_sources(test_geometry_chamfer PRIVATE tests/test_dressup_groove_end.cpp)
target_link_libraries(test_geometry_chamfer PRIVATE mitcad_geometry_analysis)
add_test(NAME geometry.dressup_groove_end COMMAND test_geometry_chamfer dressup_groove_end)
set_tests_properties(geometry.dressup_groove_end PROPERTIES TIMEOUT 300)
