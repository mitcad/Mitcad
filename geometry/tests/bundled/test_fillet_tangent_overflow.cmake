# SPDX-License-Identifier: MIT
# A rounding whose contact runs off a narrow face across its tangent edge
# with a rounded rim keeps to that face's surface, cut by the rim, with
# the port's patch 0870 (mitcad#343).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_tangent_overflow.cpp)
add_test(NAME geometry.fillet_tangent_overflow COMMAND test_geometry_fillet_t_z fillet_tangent_overflow)
