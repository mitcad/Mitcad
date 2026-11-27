# SPDX-License-Identifier: MIT
# Continuity Analysis (mitcad#2521): a box's edges G0 only (90 degrees), a
# tangent rounding's long edges G1 (0 degrees, the curvature across jumping
# by 1 / radius) and not G2, a curvature continuous rounding's G2 within
# the default tolerances, a cylinder's seam G2, a lone face's free edges not
# measured, the class by the tolerances.
target_sources(test_geometry_base PRIVATE tests/test_edge_continuity.cpp)
add_test(NAME geometry.edge_continuity COMMAND test_geometry_base edge_continuity)
set_tests_properties(geometry.edge_continuity PROPERTIES TIMEOUT 300)
