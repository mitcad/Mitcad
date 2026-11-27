# SPDX-License-Identifier: MIT
# Boxes over the trimmed faces, not their surfaces' parameter rectangles:
# synthetic faces of known extent, and a rounded bend's corner plate on a
# case of MITCAD_GEOMETRY_CASES against a fine triangulation (mitcad#2020).
target_sources(test_geometry_surfaces PRIVATE tests/test_trimmed_box.cpp)
target_link_libraries(test_geometry_surfaces PRIVATE TKMesh)
add_test(NAME geometry.trimmed_box COMMAND test_geometry_surfaces trimmed_box)
set_tests_properties(geometry.trimmed_box PROPERTIES TIMEOUT 300)
