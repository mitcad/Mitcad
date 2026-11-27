# SPDX-License-Identifier: MIT
# The roof slab's eave (mitcad#1500, mitcad#1530, the port's patches 1650
# to 1653): its two long edges and its end edges rounded with 0.9 and 1,
# the tip's rounding rolling on the obtuse edge; an end face's strip along
# a slope and the eave's end edge; the tip's corner with the slope's end
# edge. Valid closed solids with checked volumes.
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_eave.cpp)
add_test(NAME geometry.fillet_eave COMMAND test_geometry_fillet_a_e fillet_eave)
set_tests_properties(geometry.fillet_eave PROPERTIES TIMEOUT 600)
