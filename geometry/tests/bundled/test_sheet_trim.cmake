# SPDX-License-Identifier: MIT
# Sheet trims: planar booleans and exact curve transport across the sheet
# chart's seam.
target_sources(test_geometry_sheet PRIVATE tests/test_sheet_trim.cpp)
add_test(NAME geometry.sheet_trim COMMAND test_geometry_sheet sheet_trim)
