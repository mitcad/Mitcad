# SPDX-License-Identifier: MIT
# Sheet faces sampled over their surface's parameters (mitcad#2442): a
# cylinder's, a cone's (its apex too) and a box's face sampled: points,
# normals out of the body, areas; a name of no face refused; the box top's
# and the cylinder side's boundaries (mitcad#2558).
target_sources(test_geometry_sheet PRIVATE tests/test_sheet_face_grid.cpp)
add_test(NAME geometry.sheet_face_grid COMMAND test_geometry_sheet sheet_face_grid)
set_tests_properties(geometry.sheet_face_grid PROPERTIES TIMEOUT 120)
