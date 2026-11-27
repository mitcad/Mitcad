# SPDX-License-Identifier: MIT
# A pipe's Corners, mitered or rounded, along an L-shaped path
# (mitcad#1448).
target_sources(test_geometry_loft_shell PRIVATE tests/test_pipe_corners.cpp)
target_link_libraries(test_geometry_loft_shell PRIVATE mitcad_geometry_analysis)
add_test(NAME geometry.pipe_corners COMMAND test_geometry_loft_shell pipe_corners)
