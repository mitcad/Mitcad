# SPDX-License-Identifier: MIT
# Roundings round a thread's chamfer and up to a groove's floor on bodies
# with the traits of stored ones, with the port's patches 0800 and later
# (mitcad#343).
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_crest_corners.cpp)
add_test(NAME geometry.fillet_crest_corners COMMAND test_geometry_fillet_a_e fillet_crest_corners)
