# SPDX-License-Identifier: MIT
# The same fin's top rounded along its outer and its inner edges, two
# chains whose blends lie on the one top face, with OCCT alone: not
# judged overlapping (port patch 1710); with the foot's rounding as well
# the fillet returns (1711) (mitcad#1926).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_top_chains.cpp)
add_test(NAME geometry.fillet_top_chains COMMAND test_geometry_fillet_t_z fillet_top_chains)
