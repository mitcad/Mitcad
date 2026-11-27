# SPDX-License-Identifier: MIT
# A wall's edges rounded through corners where the ball stays put in the
# tube of an earlier rounding of its own radius: the walk goes on into
# that ball, and two contours meet at its section, on a synthetic block
# and on a case of MITCAD_GEOMETRY_CASES, with the port's patches 1600 and
# 1601 (mitcad#1427).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_still_ball_corners.cpp)
add_test(NAME geometry.fillet_still_ball_corners COMMAND test_geometry_fillet_o_s fillet_still_ball_corners)
set_tests_properties(geometry.fillet_still_ball_corners PROPERTIES TIMEOUT 600)
