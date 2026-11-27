# SPDX-License-Identifier: MIT
# Two roundings over a face narrower than both, the smaller ball rolling
# tangent on the larger rounding's blend, which its contact cuts, the face
# vanishing: convex with convex, concave with convex, on synthetic prisms
# (port patch 1720, mitcad#1926); and from an end to a crossing of their
# contacts only (port patch 1721).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_tangent_ride.cpp)
add_test(NAME geometry.fillet_tangent_ride COMMAND test_geometry_fillet_t_z fillet_tangent_ride)
