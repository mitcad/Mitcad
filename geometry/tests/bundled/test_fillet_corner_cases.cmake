# SPDX-License-Identifier: MIT
# Corners and ends of roundings, with the port's patches 0720 and later
# (mitcad#343).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_corner_cases.cpp)
add_test(NAME geometry.fillet_corner_cases COMMAND test_geometry_fillet_a_e fillet_corner_cases)
