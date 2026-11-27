# SPDX-License-Identifier: MIT
# A plate's side edge rounded down to a sloped face past a groove beside
# the slot's wall, on pieces of the stored body and of the import's body
# (the same faces parametrised otherwise) of a case of
# MITCAD_GEOMETRY_CASES, with the port's patch 1460 (mitcad#1395).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_end_reversed_faces.cpp)
add_test(NAME geometry.fillet_end_reversed_faces COMMAND test_geometry_fillet_a_e fillet_end_reversed_faces)
set_tests_properties(geometry.fillet_end_reversed_faces PROPERTIES TIMEOUT 600)
