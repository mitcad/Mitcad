# SPDX-License-Identifier: MIT
# Both rims of a disc thinner than the two roundings, the rims' circles
# turning opposite ways, with the port's patch 0930 (mitcad#343).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_ride_against.cpp)
add_test(NAME geometry.fillet_ride_against COMMAND test_geometry_fillet_o_s fillet_ride_against)
