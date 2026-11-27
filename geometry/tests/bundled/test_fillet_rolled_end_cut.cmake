# SPDX-License-Identifier: MIT
# A rounding rolling on an edge, its ends cut by the faces at the ends past
# where its ball comes tangent to that edge's face, with the port's patch
# 0990 (mitcad#343).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_rolled_end_cut.cpp)
add_test(NAME geometry.fillet_rolled_end_cut COMMAND test_geometry_fillet_o_s fillet_rolled_end_cut)
