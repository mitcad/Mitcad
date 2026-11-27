# SPDX-License-Identifier: MIT
# A face taken away by the roundings of both its edges, their contacts
# along each other, with the port's patch 0950 (mitcad#343).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_consumed_strip.cpp)
add_test(NAME geometry.fillet_consumed_strip COMMAND test_geometry_fillet_a_e fillet_consumed_strip)
