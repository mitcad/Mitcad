# SPDX-License-Identifier: MIT
# A Y tube (a circle swept along a line and a tangent arc, mirrored and
# joined) whose crotch, a crease fading to tangency at both ends, is
# rounded with 6 and 50 and which is shelled 2 mm inwards with its ends
# open, without a rounding and after each; a shell the kernel and the
# cavity both refuse fails in words (mitcad#2140, port patch 2140).
# `test_geometry_booleans y_tube_crotch --probe` prints the kernel's reports instead.
target_sources(test_geometry_booleans PRIVATE tests/test_y_tube_crotch.cpp)
add_test(NAME geometry.y_tube_crotch COMMAND test_geometry_booleans y_tube_crotch)
set_tests_properties(geometry.y_tube_crotch PROPERTIES TIMEOUT 600)
