# SPDX-License-Identifier: MIT
# A boss's top rim rounded down to its foot's earlier rounding where the
# foot has one: the two roundings meet along that rounding's edge, with
# the port's patch 1560 (mitcad#1419).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_rim_down_to_foot.cpp)
add_test(NAME geometry.fillet_rim_down_to_foot COMMAND test_geometry_fillet_o_s fillet_rim_down_to_foot)
set_tests_properties(geometry.fillet_rim_down_to_foot PROPERTIES TIMEOUT 600)
