# SPDX-License-Identifier: MIT
# The import's analysis queries as operations: a crash inside OCCT is the
# query's error, not the end of the import process (mitcad#2294).
target_sources(test_core_bridge PRIVATE tests/test_analysis_crash.cpp)
add_test(NAME core.analysis_crash_boundary_distances COMMAND test_core_bridge analysis_crash boundary-distances)
set_tests_properties(core.analysis_crash_boundary_distances PROPERTIES
  ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=boundary distances" TIMEOUT 120)
add_test(NAME core.analysis_crash_points_inside COMMAND test_core_bridge analysis_crash points-inside)
set_tests_properties(core.analysis_crash_points_inside PROPERTIES
  ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=points inside" TIMEOUT 120)
