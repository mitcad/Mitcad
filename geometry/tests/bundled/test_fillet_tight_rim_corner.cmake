# SPDX-License-Identifier: MIT
# A face's outline rounded past a cylinder's rims, or a wall's corners,
# rounded with less than the radius where the other wall's edge with the
# face is acute (the vertex blend and a torus) or obtuse (the rim's own
# ball rolled on between the roundings), on a synthetic cut tube and block
# and a case of MITCAD_GEOMETRY_CASES, with the port's patch 1590
# (mitcad#1426).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_tight_rim_corner.cpp)
add_test(NAME geometry.fillet_tight_rim_corner COMMAND test_geometry_fillet_t_z fillet_tight_rim_corner)
set_tests_properties(geometry.fillet_tight_rim_corner PROPERTIES TIMEOUT 600)
