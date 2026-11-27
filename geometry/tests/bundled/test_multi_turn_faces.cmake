# SPDX-License-Identifier: MIT
# Booleans on stored faces that run many turns (threaded holes, mitcad#349).
target_sources(test_geometry_booleans PRIVATE tests/test_multi_turn_faces.cpp)
add_test(NAME geometry.multi_turn_faces COMMAND test_geometry_booleans multi_turn_faces)
