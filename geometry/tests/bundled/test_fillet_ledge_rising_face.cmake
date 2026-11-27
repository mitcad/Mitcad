# SPDX-License-Identifier: MIT
# A rounding wider than the ledge beside its edge, the face beyond rising
# from the ledge (GitHub #620's simpler case, mitcad#2352): a block with a
# spike whose foot is 1 in from its sides, its front top edge rounded with
# 1.5, 2 and 4: one valid solid, the rounding tangent to the block's
# front and the spike's front continued down, the volume exact.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_ledge_rising_face.cpp)
add_test(NAME geometry.fillet_ledge_rising_face COMMAND test_geometry_fillet_f_n fillet_ledge_rising_face)
set_tests_properties(geometry.fillet_ledge_rising_face PROPERTIES TIMEOUT 300)
