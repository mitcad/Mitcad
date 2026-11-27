# SPDX-License-Identifier: MIT
# Flex routes (mitcad#1713): a hose over the edge between two faces of a
# block and round the block between opposite faces, through the air round
# it, along a cylinder; a flat cable's flat side on the cylinder and a
# half turn at a node; a turn the item cannot make returned with its
# places; the solids' volumes; the ports of a route and of a pipe's end;
# a wire from a pipe's end over a block's top and edge to its side, and
# its hollow solid.
target_sources(test_geometry_sheet PRIVATE tests/test_flex_route.cpp)
add_test(NAME geometry.flex_route COMMAND test_geometry_sheet flex_route)
# Its cases run 4-5 minutes under a release's load (238.5 s in 0.65.0,
# past 300 s in 0.66.0-rc1 with the added cases).
set_tests_properties(geometry.flex_route PROPERTIES TIMEOUT 900)
