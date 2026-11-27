# SPDX-License-Identifier: MIT
# Emboss: profiles wrapped onto cylinders (mitcad#458).
target_sources(test_geometry_sheet PRIVATE tests/test_emboss.cpp)
add_test(NAME geometry.emboss COMMAND test_geometry_sheet emboss)
