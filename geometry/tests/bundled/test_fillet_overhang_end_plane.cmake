# SPDX-License-Identifier: MIT
# An overhang's bottom edge rounded from where an earlier rounding of the
# ledge's edge on the same line ends (GitHub #572, mitcad#2276): ended flat
# on the lower block's end plane at 3, 5, 8 and 12, one valid closed solid,
# the earlier rounding's faces unchanged; a radius too large fails in
# words or builds, never that the result is not a closed solid.
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_overhang_end_plane.cpp)
add_test(NAME geometry.fillet_overhang_end_plane COMMAND test_geometry_fillet_o_s fillet_overhang_end_plane)
set_tests_properties(geometry.fillet_overhang_end_plane PROPERTIES TIMEOUT 300)
