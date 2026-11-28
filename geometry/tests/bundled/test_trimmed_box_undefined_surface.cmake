# SPDX-License-Identifier: MIT
# The box of a face whose surface has no value beyond it, where OCCT's
# optimal box fails, and what Section Analysis tells from it: planes that
# cross the face or pass it (mitcad#2633).
target_sources(test_geometry_surfaces PRIVATE tests/test_trimmed_box_undefined_surface.cpp)
target_link_libraries(test_geometry_surfaces PRIVATE mitcad_geometry_analysis)
add_test(NAME geometry.trimmed_box_undefined_surface COMMAND test_geometry_surfaces trimmed_box_undefined_surface)
set_tests_properties(geometry.trimmed_box_undefined_surface PROPERTIES TIMEOUT 120)
