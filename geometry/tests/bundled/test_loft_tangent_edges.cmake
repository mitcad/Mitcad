# SPDX-License-Identifier: MIT
# Tangent Edges: Merge (mitcad#2162): a circle R30 lofted to a 160 x 40
# slot 50 mm above it is one valid side face, tangent-continuous (G1)
# across the slot's former line-arc joins and the seam, two sections, three
# (Hermite) and with a tangent end condition (skinned); kept, a face per
# edge; a rectangle keeps its corner edges; the curve sections' surface
# loft likewise.
target_sources(test_geometry_loft_shell PRIVATE tests/test_loft_tangent_edges.cpp)
add_test(NAME geometry.loft_tangent_edges COMMAND test_geometry_loft_shell loft_tangent_edges)
set_tests_properties(geometry.loft_tangent_edges PROPERTIES TIMEOUT 300)
