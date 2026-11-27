# SPDX-License-Identifier: MIT
# Sheet bends: the material rebuilt where a circular cut crosses both tangent
# seams of an L.
target_sources(test_geometry_sheet PRIVATE tests/test_sheet_bend.cpp)
add_test(NAME geometry.sheet_bend COMMAND test_geometry_sheet sheet_bend)
