# SPDX-License-Identifier: MIT
# Lofts from a polygon to a circle shelled with both ends open and offset
# closed, with the port's patches 1130 and 1190 (mitcad#1184, mitcad#1193).
# One ctest per loft: in one ctest the cases took 158 s of 300 and timed
# out under a release's load (mitcad#1565).
target_sources(test_geometry_loft_shell PRIVATE tests/test_shell_loft_to_circle.cpp)
set(shell_loft_to_circle_parts
  octagon_24_5_seam_0
  octagon_24_5_seam_22_5
  octagon_21_7_seam_0
  octagon_21_7_seam_22_5
  square_20
  lean_in_8
  lean_in_10
  lean_in_14_1
  facade)
foreach(part IN LISTS shell_loft_to_circle_parts)
  add_test(NAME geometry.shell_loft_to_circle.${part} COMMAND test_geometry_loft_shell shell_loft_to_circle ${part})
  set_tests_properties(geometry.shell_loft_to_circle.${part} PROPERTIES TIMEOUT 300)
endforeach()
add_test(NAME geometry.shell_loft_to_circle_parts
  COMMAND test_geometry_loft_shell shell_loft_to_circle --check-parts ${shell_loft_to_circle_parts})
