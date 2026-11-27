# SPDX-License-Identifier: MIT
# Lofts through two smooth closed sections, Tangent Edges Keep and Merge
# (mitcad#2271): circles to slots of several proportions (the owner's R111
# to 520 x 80 among them), an ellipse to a rounded rectangle, a circle to
# a spline outline, shelled Inside, Outside and Both, Sharp and Rounded,
# walls at the thickness within 1e-6 mm (Keep through one-edge sections
# too, mitcad#2308); the owner's surface loft and the circle to spline
# outline as a surface thickened. One ctest per loft and Keep or Merge.
target_sources(test_geometry_loft_shell PRIVATE tests/test_loft_smooth_sections_shell.cpp)
set(loft_smooth_sections_shell_parts
  circle_slot_r30_keep
  circle_slot_r30_merge
  circle_slot_r50_keep
  circle_slot_r50_merge
  circle_slot_owner_keep
  circle_slot_owner_merge
  circle_slot_long_keep
  circle_slot_long_merge
  ellipse_rounded_rectangle_keep
  ellipse_rounded_rectangle_merge
  circle_spline_keep
  circle_spline_merge
  thicken_owner_keep
  thicken_owner_merge
  thicken_circle_spline_keep
  thicken_circle_spline_merge)
foreach(part IN LISTS loft_smooth_sections_shell_parts)
  add_test(NAME geometry.loft_smooth_sections_shell.${part} COMMAND test_geometry_loft_shell loft_smooth_sections_shell ${part})
  set_tests_properties(geometry.loft_smooth_sections_shell.${part} PROPERTIES TIMEOUT 600)
endforeach()
add_test(NAME geometry.loft_smooth_sections_shell_parts
  COMMAND test_geometry_loft_shell loft_smooth_sections_shell --check-parts ${loft_smooth_sections_shell_parts})
