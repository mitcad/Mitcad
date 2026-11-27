# SPDX-License-Identifier: MIT
# A block's foot edges on a plate rounded (mitcad#1458): three of them
# meet in mitres at the block's convex corners without the concave
# check's warning of a rounding hanging out; with the ends' alone, the
# front's foot edge in the result comes from the foot edge before
# (item_origin), the roundings are the fillet's own.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_foot_mitre.cpp)
add_test(NAME geometry.fillet_foot_mitre COMMAND test_geometry_fillet_f_n fillet_foot_mitre)
