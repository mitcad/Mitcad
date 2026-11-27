# SPDX-License-Identifier: MIT
# Projection bounds whose parameter tolerance is below floating-point
# spacing, with the port's patch 0960 (mitcad#504).
target_sources(test_geometry_surfaces PRIVATE tests/test_projection_boundary.cpp)
add_test(NAME geometry.projection_boundary COMMAND test_geometry_surfaces projection_boundary)
set_tests_properties(geometry.projection_boundary PROPERTIES TIMEOUT 30)
