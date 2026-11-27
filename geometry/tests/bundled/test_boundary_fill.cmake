# SPDX-License-Identifier: MIT
# Boundary Fill: the cells tool bodies enclose (mitcad#457).
target_sources(test_geometry_surfaces PRIVATE tests/test_boundary_fill.cpp)
add_test(NAME geometry.boundary_fill COMMAND test_geometry_surfaces boundary_fill)
