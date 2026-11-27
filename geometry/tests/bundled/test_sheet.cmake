# SPDX-License-Identifier: MIT
# Planar material domains and sheet thickness/reference sides (mitcad#206).
target_sources(test_geometry_sheet PRIVATE tests/test_sheet.cpp)
add_test(NAME geometry.sheet COMMAND test_geometry_sheet sheet)
