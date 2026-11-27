# SPDX-License-Identifier: MIT
# Reported volumes and areas against exact values and OCCT's adaptive
# Gauss-Kronrod integration, lofts and their shells included (mitcad#1200).
target_sources(test_geometry_surfaces PRIVATE tests/test_accurate_volumes.cpp)
target_link_libraries(test_geometry_surfaces PRIVATE TKMesh)
add_test(NAME geometry.accurate_volumes COMMAND test_geometry_surfaces accurate_volumes)
set_tests_properties(geometry.accurate_volumes PROPERTIES TIMEOUT 300)
