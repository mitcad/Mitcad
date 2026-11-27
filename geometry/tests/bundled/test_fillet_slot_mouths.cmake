# SPDX-License-Identifier: MIT
# A rounding over the mouths of slots whose rounded ends touch the top
# along a crest line, on a case of MITCAD_GEOMETRY_CASES, with the port's
# patch 1210 (mitcad#1204).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_slot_mouths.cpp)
add_test(NAME geometry.fillet_slot_mouths COMMAND test_geometry_fillet_o_s fillet_slot_mouths)
set_tests_properties(geometry.fillet_slot_mouths PROPERTIES TIMEOUT 600)
