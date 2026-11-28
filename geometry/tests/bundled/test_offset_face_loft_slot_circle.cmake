# SPDX-License-Identifier: MIT
# Offset Face on the outer wall of a hollow loft from a slot to a circle
# (mitcad#2546, GitHub #723): the reported design's loft, shelled 2 mm
# inside with both ends removed, its outer wall moved 2 mm out with Tangent
# Chain; the inner wall, which the kernel counts as only C0, stays on its
# surface.
target_sources(test_geometry_loft_shell PRIVATE tests/test_offset_face_loft_slot_circle.cpp)
add_test(NAME geometry.offset_face_loft_slot_circle COMMAND test_geometry_loft_shell offset_face_loft_slot_circle)
set_tests_properties(geometry.offset_face_loft_slot_circle PROPERTIES TIMEOUT 600)
