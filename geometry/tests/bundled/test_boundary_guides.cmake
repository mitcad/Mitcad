# SPDX-License-Identifier: MIT
# Boundary Surface's Fill a Loop through guide curves inside it (mitcad#2526).
target_sources(test_geometry_surfaces PRIVATE tests/test_boundary_guides.cpp)
add_test(NAME geometry.boundary_guides COMMAND test_geometry_surfaces boundary_guides)
