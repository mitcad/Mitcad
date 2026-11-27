# SPDX-License-Identifier: MIT
# A rounding along an edge of a strip the ball cannot touch, with the
# port's patch 0880 (mitcad#343).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_strip_start.cpp)
add_test(NAME geometry.fillet_strip_start COMMAND test_geometry_fillet_o_s fillet_strip_start)
