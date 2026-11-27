# SPDX-License-Identifier: MIT
# A block's foot and sloping top rounded beside a front face lower than
# the two roundings together: their contacts cross before ends joined by
# the front face's edge; reported as overlapping and built one after the
# other, alone and against a wall whose foot is rounded too (port patch
# 1780, mitcad#1987).
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_linked_ends.cpp)
add_test(NAME geometry.fillet_linked_ends COMMAND test_geometry_fillet_f_n fillet_linked_ends)
