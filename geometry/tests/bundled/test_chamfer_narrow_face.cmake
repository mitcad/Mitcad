# SPDX-License-Identifier: MIT
# Chamfers wider than a narrow face beside their edge, cut by the face
# beyond (mitcad#1232).
target_sources(test_geometry_chamfer PRIVATE tests/test_chamfer_narrow_face.cpp)
target_link_libraries(test_geometry_chamfer PRIVATE mitcad_geometry_analysis)
add_test(NAME geometry.chamfer_narrow_face COMMAND test_geometry_chamfer chamfer_narrow_face)
