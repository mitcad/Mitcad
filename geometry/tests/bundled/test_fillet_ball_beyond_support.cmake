# SPDX-License-Identifier: MIT
# A drafted block's rim rounded with a ball larger than the roundings of
# its corners, the ball's centre beyond their axes: walked with its
# sections in the guide's normal plane, with OCCT alone (port patch 1760,
# mitcad#1977).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_ball_beyond_support.cpp)
add_test(NAME geometry.fillet_ball_beyond_support COMMAND test_geometry_fillet_a_e fillet_ball_beyond_support)
