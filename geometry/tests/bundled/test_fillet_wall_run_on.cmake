# SPDX-License-Identifier: MIT
# A fin's foot rounded higher than the flat stretch its curved side bends
# into, its chain ending where the fin's top runs down to the plate: the
# blend runs on over the flat stretch's plane, cut by the top, with OCCT
# alone, on a case of MITCAD_GEOMETRY_CASES (port patch 1700,
# mitcad#1926).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_wall_run_on.cpp)
add_test(NAME geometry.fillet_wall_run_on COMMAND test_geometry_fillet_t_z fillet_wall_run_on)
