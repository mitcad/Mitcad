# SPDX-License-Identifier: MIT
# A rounding whose edge ends where a convex crease of its face begins:
# the ball rolled on over the crease into the face at the end (a horn
# torus), on a synthetic cut cylinder and a case of MITCAD_GEOMETRY_CASES,
# with the port's patch 1620 (mitcad#1434).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_crease_end_corner.cpp)
add_test(NAME geometry.fillet_crease_end_corner COMMAND test_geometry_fillet_a_e fillet_crease_end_corner)
set_tests_properties(geometry.fillet_crease_end_corner PROPERTIES TIMEOUT 600)
