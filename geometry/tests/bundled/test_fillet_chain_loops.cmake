# SPDX-License-Identifier: MIT
# A fillet contour round a closed chain that stops at a turn short of its
# start ends, with the port's patch 0840 (mitcad#434).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_chain_loops.cpp)
add_test(NAME geometry.fillet_chain_loops COMMAND test_geometry_fillet_a_e fillet_chain_loops)
