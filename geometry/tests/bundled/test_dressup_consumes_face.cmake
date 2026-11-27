# SPDX-License-Identifier: MIT
# A fillet or chamfer wider than a face beside its edge takes that face
# away (mitcad#1819), one table, one row per case, over the narrow face
# (planar between parallel or other faces, a cylinder, a cone, a
# B-spline strip), the chain (one edge, a tangent chain over rounded
# corners, an octagon over sharp corners, an edge ending on a third face),
# convex and concave, the dressup (chamfers: equal, two distances each
# way, distance and angle; fillets) and the size (0.5w to 3w, and one past
# the next face, which fails in words): the equal chamfer and the fillet
# at every size on every narrow face, every dressup at the exact width,
# 1.01w and past, and pairwise rows that put every two columns' values
# together; each one valid closed solid whose volume is the reference's
# built in the owner's order (the dressup on the picked edge with the
# narrow face continued, then the face beyond grown out), exactly where
# the face is planar along a straight chain. Three parts, one ctest each,
# and a ring's floor chamfered inside a rounded octagon (mitcad#1821, `ring`).
target_sources(test_geometry_chamfer PRIVATE tests/test_dressup_consumes_face.cpp)
target_link_libraries(test_geometry_chamfer PRIVATE mitcad_geometry_analysis)
foreach(part IN ITEMS core kinds chains ring)
  add_test(NAME geometry.dressup_consumes_face.${part} COMMAND test_geometry_chamfer dressup_consumes_face --part ${part})
  set_tests_properties(geometry.dressup_consumes_face.${part} PROPERTIES TIMEOUT 300)
endforeach()
