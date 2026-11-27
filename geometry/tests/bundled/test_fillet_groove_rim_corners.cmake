# SPDX-License-Identifier: MIT
# A concave groove's rim rounded past an earlier rounding of the ball's
# radius and up to a wall's arc: the walk on from the standing ball's
# sphere where the guide's normal planes pass it forwards, the corners
# where the rim's roundings cross cut along their intersection, on a
# synthetic leg and a case of MITCAD_GEOMETRY_CASES, with the port's
# patches 1610 and 1611 (mitcad#1428).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_groove_rim_corners.cpp)
add_test(NAME geometry.fillet_groove_rim_corners COMMAND test_geometry_fillet_f_n fillet_groove_rim_corners)
set_tests_properties(geometry.fillet_groove_rim_corners PROPERTIES TIMEOUT 600)
