# SPDX-License-Identifier: MIT
# Pieces of a face a fillet takes away, left between two of its own blend
# faces, closed by a patch at each place (mitcad#1858): the patch on a
# block's rounded corner (one valid closed solid, the names kept, the
# volume within 1 %), and two bodies like a reported design's places (a
# slot's rim on an earlier rounding, over rounded corners; a cut face's
# rim passing from a rounding onto the faces beside it) rounded with 5:
# one valid closed solid, no warning, no piece left, within 20 s.
target_sources(test_geometry_chamfer PRIVATE tests/test_dressup_blend_remnants.cpp)
add_test(NAME geometry.dressup_blend_remnants COMMAND test_geometry_chamfer dressup_blend_remnants)
set_tests_properties(geometry.dressup_blend_remnants PROPERTIES TIMEOUT 300)
