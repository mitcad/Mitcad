# SPDX-License-Identifier: MIT
# Nearness told without measuring the whole distance (mitcad#1785):
# within_distance agrees with BRepExtrema's distance at limits around it
# for solids, faces (a B-spline one too), edges and vertices apart,
# touching and inside, and a surface probe with a projection of each
# point on B-spline, cylindrical and plane surfaces.
target_sources(test_geometry_surfaces PRIVATE tests/test_proximity.cpp)
add_test(NAME geometry.proximity COMMAND test_geometry_surfaces proximity)
