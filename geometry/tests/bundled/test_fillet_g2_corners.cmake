# SPDX-License-Identifier: MIT
# Curvature continuous (G2) fillet corners (mitcad#2332, GitHub
# mitcad/Mitcad#611): a box's corner of three 4 mm G2 edges with Rolling
# Ball and Setback corners, and all twelve edges, the corner patch
# within 0.01 degree and 1 % of curvature along its boundary; two G2 sets
# meeting a G1 set; a thin box; two G2 edges at a corner whose third edge
# stays sharp meeting in a mitre; an asymmetric set at the corner refused
# in words.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_g2_corners.cpp)
add_test(NAME geometry.fillet_g2_corners COMMAND test_geometry_fillet_f_n fillet_g2_corners)
set_tests_properties(geometry.fillet_g2_corners PROPERTIES TIMEOUT 300)
