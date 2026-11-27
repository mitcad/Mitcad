# SPDX-License-Identifier: MIT
# A roof slab's section of 7 points extruded and rounded on all its edges
# (mitcad#1482): it builds, or the error names the face whose two edges'
# roundings reach across it.
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_roof_slab.cpp)
add_test(NAME geometry.fillet_roof_slab COMMAND test_geometry_fillet_o_s fillet_roof_slab)
set_tests_properties(geometry.fillet_roof_slab PROPERTIES TIMEOUT 600)
