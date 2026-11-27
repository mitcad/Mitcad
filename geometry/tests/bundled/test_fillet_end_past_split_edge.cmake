# SPDX-License-Identifier: MIT
# A fillet's end cut by the faces round a notch (mitcad#1615, the port's
# patch 1670): a flange's foot rounded past an end cap that stops short of
# the flange's tip, the rounding crossing the cap's side's boundary with
# the flange's inside past a vertex that splits it: one valid closed solid,
# every edge on two faces, the exact shape's volume.
target_sources(test_geometry_fillet_a_e PRIVATE tests/test_fillet_end_past_split_edge.cpp)
add_test(NAME geometry.fillet_end_past_split_edge COMMAND test_geometry_fillet_a_e fillet_end_past_split_edge)
