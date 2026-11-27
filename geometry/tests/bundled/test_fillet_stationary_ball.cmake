# SPDX-License-Identifier: MIT
# A lid's rims rounded through the mitres of earlier roundings of the same
# radius, the ball staying put at each corner, on synthetic lids and a
# case of MITCAD_GEOMETRY_CASES, with the port's patches 1520 and 1521
# (mitcad#1412).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_stationary_ball.cpp)
add_test(NAME geometry.fillet_stationary_ball COMMAND test_geometry_fillet_o_s fillet_stationary_ball)
set_tests_properties(geometry.fillet_stationary_ball PROPERTIES TIMEOUT 600)
