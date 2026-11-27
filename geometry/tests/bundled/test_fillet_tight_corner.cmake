# SPDX-License-Identifier: MIT
# A bottom outline rounded past wall corners rounded with less than its
# radius, with the port's patch 0970 (mitcad#343).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_tight_corner.cpp)
add_test(NAME geometry.fillet_tight_corner COMMAND test_geometry_fillet_t_z fillet_tight_corner)
