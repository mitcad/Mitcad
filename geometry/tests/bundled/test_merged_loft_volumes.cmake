# SPDX-License-Identifier: MIT
# The reported volume and area of lofts with Tangent Edges Merge (one
# side face on a periodic surface joined C1) and of their 2 mm shells
# against OCCT's Gauss-Kronrod integration of a copy split at its C1 lines
# within 1e-4 and a mesh of 5e-3 mm within 5e-3 (mitcad#2309): circles
# to slots, the owner's among them, and a circle to a spline outline. One
# ctest per loft, each prints what its measures cost.
target_sources(test_geometry_loft_shell PRIVATE tests/test_merged_loft_volumes.cpp)
target_link_libraries(test_geometry_loft_shell PRIVATE mitcad_geometry_analysis TKMesh)
foreach(part IN ITEMS circle_slot_r50 circle_slot_r30 circle_spline circle_slot_owner)
  add_test(NAME geometry.merged_loft_volumes.${part} COMMAND test_geometry_loft_shell merged_loft_volumes ${part})
  set_tests_properties(geometry.merged_loft_volumes.${part} PROPERTIES TIMEOUT 300)
endforeach()
