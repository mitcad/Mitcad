# SPDX-License-Identifier: MIT
# OCCT's solid classifier on a point in the plane of a far face of the
# shell, and the check of a block whose two cavities' bottoms lie in one
# plane, with the port's patch 1285 (mitcad#1280); a point in the plane of
# a flat shell and a cut by such a shell (mitcad#1881).
target_sources(test_geometry_booleans PRIVATE tests/test_classifier_far_plane.cpp)
add_test(NAME geometry.classifier_far_plane COMMAND test_geometry_booleans classifier_far_plane)
