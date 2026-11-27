# SPDX-License-Identifier: MIT
# Shell with faces of their own wall thickness (mitcad#2443, GitHub
# mitcad/Mitcad#642): a box's bottom at its own thickness Inside, Outside,
# Both, closed and Rounded inside, a cylinder's side, a rounded edge's
# tangent chain taking the thickness, exact volumes and wall planes;
# Rounded outside, closed and Both, the roundings running from one
# thickness to the other measured tangent (mitcad#2488, port patch 2488);
# the refusals (a rounded corner of walls not square to each other,
# without the tangent chain, a removed face, a wall the others cannot
# meet).
target_sources(test_geometry_loft_shell PRIVATE tests/test_shell_own_thickness.cpp)
add_test(NAME geometry.shell_own_thickness COMMAND test_geometry_loft_shell shell_own_thickness)
set_tests_properties(geometry.shell_own_thickness PROPERTIES TIMEOUT 300)
