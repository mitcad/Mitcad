# SPDX-License-Identifier: MIT
# Mitred elbows and eccentric cones as sheet panels (mitcad#2491): the
# three gores of a 90-degree elbow developed with their cuts at the
# neutral surface (the stretch-out at both seams, the heel and the
# throat), an eccentric frustum developed by its generators (its edges'
# lengths and its area at the neutral surface), also with its side two
# faces developed as one panel (mitcad#2559), a twisted ruled surface
# refused.
target_sources(test_geometry_sheet PRIVATE tests/test_sheet_mitred_oblique.cpp)
add_test(NAME geometry.sheet_mitred_oblique COMMAND test_geometry_sheet sheet_mitred_oblique)
set_tests_properties(geometry.sheet_mitred_oblique PROPERTIES TIMEOUT 300)
