# SPDX-License-Identifier: MIT
# The rim between a loft's side and a cylinder pulled from its end face,
# convex in some places and concave in others, rounded (mitcad#2300): the
# owner's case at 5 mm, at 2 and 10 mm and with pulls of 50 and 400 mm,
# Tangent Edges Merge and Keep; one valid solid, the whole rim rounded,
# the roundings vanishing where its faces are tangent. The kernel_ parts:
# the body the fillet rounds (joined, split) rounded by the kernel's fillet
# alone with 2, 5 and 10 (OCCT port patches 2300, 2301, and 2302, 2303 for
# Keep at 2 mm). One ctest per part.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_loft_rim.cpp)
foreach(part IN ITEMS owner_merge owner_keep radii_merge radii_keep pulls_merge pulls_keep far_end_unsplit
                      kernel_merge kernel_keep)
  add_test(NAME geometry.fillet_loft_rim.${part} COMMAND test_geometry_fillet_f_n fillet_loft_rim ${part})
  set_tests_properties(geometry.fillet_loft_rim.${part} PROPERTIES TIMEOUT 600)
endforeach()
