# SPDX-License-Identifier: MIT
# Rolling-ball blends of imported bodies stored without their
# approximation, walked by OCCT (mitcad#621).
target_sources(test_geometry_surfaces PRIVATE tests/test_blend_surfaces.cpp)
add_test(NAME geometry.blend_surfaces COMMAND test_geometry_surfaces blend_surfaces)
