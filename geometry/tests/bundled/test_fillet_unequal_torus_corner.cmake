# SPDX-License-Identifier: MIT
# A torus corner whose sides have different radii (a block on a plate,
# its vertical edge rounded 5, the feet there 0.8 and 1), with OCCT's
# fillet alone: the torus of the larger radius, run on past its section
# up to where the smaller rounding's blend crosses it (port patch 1860),
# against the volume of the blends' fills; the pocket that is its
# complement; a box's convex corner walked as before (mitcad#2091).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_unequal_torus_corner.cpp)
add_test(NAME geometry.fillet_unequal_torus_corner COMMAND test_geometry_fillet_t_z fillet_unequal_torus_corner)
set_tests_properties(geometry.fillet_unequal_torus_corner PROPERTIES TIMEOUT 300)
