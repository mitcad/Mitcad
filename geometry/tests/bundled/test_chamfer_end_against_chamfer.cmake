# SPDX-License-Identifier: MIT
# A chamfer whose chain runs on past its start into a face beyond and ends
# against an earlier chamfer, alike at both ends, with the port's patch
# 1120 (mitcad#1185).
target_sources(test_geometry_chamfer PRIVATE tests/test_chamfer_end_against_chamfer.cpp)
add_test(NAME geometry.chamfer_end_against_chamfer COMMAND test_geometry_chamfer chamfer_end_against_chamfer)
set_tests_properties(geometry.chamfer_end_against_chamfer PROPERTIES TIMEOUT 60)
