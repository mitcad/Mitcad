# SPDX-License-Identifier: MIT
# A plate's sides rounded along an earlier rounding that vanishes into the
# side, on a case of MITCAD_GEOMETRY_CASES, with the port's patch 1320
# (mitcad#1346).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_vanishing_side.cpp)
add_test(NAME geometry.fillet_vanishing_side COMMAND test_geometry_fillet_t_z fillet_vanishing_side)
set_tests_properties(geometry.fillet_vanishing_side PROPERTIES TIMEOUT 600)
