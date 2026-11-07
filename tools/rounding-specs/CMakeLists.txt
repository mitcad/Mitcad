# SPDX-License-Identifier: MIT
# One rounding spec of the .f3d corpora run again through the dressup and
# compared with the file's body (mitcad#1398; core/import/docs/rounding-specs.md).
add_executable(mitcad-rounding-specs main.cpp)
mitcad_utf8_code_page(mitcad-rounding-specs)
target_link_libraries(mitcad-rounding-specs PRIVATE mitcad_geometry TKTopAlgo mitcad_warnings)
