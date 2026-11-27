# SPDX-License-Identifier: MIT
# Intersection curves: an arc and a line, two cylinders whose crossing
# branches, parallel normals refused (mitcad#1405).
target_sources(test_geometry_surfaces PRIVATE tests/test_intersection_curve.cpp)
add_test(NAME geometry.intersection_curve COMMAND test_geometry_surfaces intersection_curve)
