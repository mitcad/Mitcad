# SPDX-License-Identifier: MIT
# A rib's rounding ending at its foot beside a channel's rim, a vertex
# with a concave edge (the rib's end on the flat), with OCCT's fillet
# alone: the corner set back along the concave edge only, its plate
# inside the body and the flat past the rib kept (port patch 1805,
# mitcad#2038).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_rib_foot_corner.cpp)
add_test(NAME geometry.fillet_rib_foot_corner COMMAND test_geometry_fillet_o_s fillet_rib_foot_corner)
set_tests_properties(geometry.fillet_rib_foot_corner PROPERTIES TIMEOUT 300)
