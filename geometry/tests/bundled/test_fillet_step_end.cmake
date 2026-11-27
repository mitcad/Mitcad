# SPDX-License-Identifier: MIT
# A rounding ending where its wall steps back by much less than the
# radius, with the port's patch 0951 (mitcad#343).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_step_end.cpp)
add_test(NAME geometry.fillet_step_end COMMAND test_geometry_fillet_o_s fillet_step_end)
