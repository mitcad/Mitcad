# SPDX-License-Identifier: MIT
# A box's vertical edge rounded up through the mitre of two earlier
# roundings of its top edges of the same radius, the spine either way,
# with the port's patch 1490 (mitcad#1397).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_mitre_same_radius.cpp)
add_test(NAME geometry.fillet_mitre_same_radius COMMAND test_geometry_fillet_f_n fillet_mitre_same_radius)
set_tests_properties(geometry.fillet_mitre_same_radius PROPERTIES TIMEOUT 600)
