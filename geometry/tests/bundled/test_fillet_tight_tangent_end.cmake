# SPDX-License-Identifier: MIT
# A hub's foot rounded past the earlier roundings of the rims of holes
# that touch the hub, tighter than the ball: the ball rolled on over the
# rim's torus, cut by the face along the foot, each end a point on the
# foot's edge, one face with the piece walked before it, on a case of
# MITCAD_GEOMETRY_CASES, with the port's patches 1530 and 1540
# (mitcad#1413, mitcad#1418).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_tight_tangent_end.cpp)
add_test(NAME geometry.fillet_tight_tangent_end COMMAND test_geometry_fillet_t_z fillet_tight_tangent_end)
set_tests_properties(geometry.fillet_tight_tangent_end PROPERTIES TIMEOUT 600)
