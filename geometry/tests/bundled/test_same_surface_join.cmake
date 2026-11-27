# SPDX-License-Identifier: MIT
# Faces on one surface joined after a cut, keeping all their names: a
# cut's wall from eight arcs of one circle, going on from a rim
# (mitcad#1261).
target_sources(test_geometry_booleans PRIVATE tests/test_same_surface_join.cpp)
add_test(NAME geometry.same_surface_join COMMAND test_geometry_booleans same_surface_join)
