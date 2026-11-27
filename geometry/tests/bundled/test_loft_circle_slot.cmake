# SPDX-License-Identifier: MIT
# A loft from a circle to a slot, its faces tangent along the lines
# between them (mitcad#2141, mitcad#2143): shelled 2 mm inside with
# either end removed, Sharp and Round, and the owner's case (valid single
# solids, the inner faces the wall from the outer ones); its surface loft
# thickened 1 mm to each side and symmetric (valid single solids, the
# volumes the area times the thickness); a square to a circle still
# OCCT's loft.
target_sources(test_geometry_loft_shell PRIVATE tests/test_loft_circle_slot.cpp)
add_test(NAME geometry.loft_circle_slot COMMAND test_geometry_loft_shell loft_circle_slot)
set_tests_properties(geometry.loft_circle_slot PROPERTIES TIMEOUT 600)
