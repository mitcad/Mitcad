# SPDX-License-Identifier: MIT
# A drafted peg's corners rounded down to a block whose walls go on from
# the peg's sides at slight concave creases and whose corners are rounded
# with less, the walls and those corners cutting the rounding, with the
# port's patch 1020 (mitcad#343).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_crease_into_end_walls.cpp)
add_test(NAME geometry.fillet_crease_into_end_walls COMMAND test_geometry_fillet_a_e fillet_crease_into_end_walls)
