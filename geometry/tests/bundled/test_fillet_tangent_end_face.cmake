# SPDX-License-Identifier: MIT
# A rounding ending where the face at the end goes on tangentially from
# one of its faces, cut by that face, with the port's patch 0911
# (mitcad#343).
target_sources(test_geometry_fillet_t_z PRIVATE tests/test_fillet_tangent_end_face.cpp)
add_test(NAME geometry.fillet_tangent_end_face COMMAND test_geometry_fillet_t_z fillet_tangent_end_face)
