# SPDX-License-Identifier: MIT
# Roundings of a short slanted corner edge where a thin wall meets a
# rounded end wall (mitcad#1253).
target_sources(test_geometry_fillet_o_s PRIVATE tests/test_fillet_step_corner.cpp)
target_link_libraries(test_geometry_fillet_o_s PRIVATE mitcad_geometry_io)
add_test(NAME geometry.fillet_step_corner COMMAND test_geometry_fillet_o_s fillet_step_corner)
set_tests_properties(geometry.fillet_step_corner PROPERTIES TIMEOUT 300)
