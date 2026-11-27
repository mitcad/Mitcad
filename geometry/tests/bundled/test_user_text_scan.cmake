# SPDX-License-Identifier: MIT
# Thrown texts say what is wrong in words (mitcad#1612): no face, edge or
# vertex named and no point printed outside placed_error() and its helpers
# (mitcad/geometry/user_error.hpp); the scan of the sources and its own
# check on planted ones.
target_sources(test_geometry_base PRIVATE tests/test_user_text_scan.cpp)
add_test(NAME geometry.user-text-scan COMMAND test_geometry_base user_text_scan "${CMAKE_CURRENT_SOURCE_DIR}/src")
add_test(NAME geometry.user-text-scan-self-test COMMAND test_geometry_base user_text_scan --self-test)
