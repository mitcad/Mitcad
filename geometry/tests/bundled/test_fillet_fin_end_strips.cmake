# SPDX-License-Identifier: MIT
# A fin's side strips, narrower than the ball, rounded against its root's
# chamfer, the ball rolled on along their edges past the transition pieces
# of the arcs' chamfer, on a case of MITCAD_GEOMETRY_CASES, with the port's
# patch 1420 (mitcad#1388).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_fin_end_strips.cpp)
add_test(NAME geometry.fillet_fin_end_strips COMMAND test_geometry_fillet_f_n fillet_fin_end_strips)
set_tests_properties(geometry.fillet_fin_end_strips PROPERTIES TIMEOUT 600)
