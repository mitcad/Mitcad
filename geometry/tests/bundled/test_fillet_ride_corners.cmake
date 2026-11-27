# SPDX-License-Identifier: MIT
# Roundings riding on another's contact line and the corners at their ends
# (mitcad#1499, the port's patches 1640 to 1642): a plate 1.3 thick
# rounded with 1 on an end face's rims, with the vertical edges at both
# its ends, and on all 12 edges; the roof slab's end-face pair along one
# slope. Valid closed solids with checked volumes.
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_ride_corners.cpp)
add_test(NAME geometry.fillet_ride_corners COMMAND test_geometry_fillet_o_s fillet_ride_corners)
set_tests_properties(geometry.fillet_ride_corners PROPERTIES TIMEOUT 600)
