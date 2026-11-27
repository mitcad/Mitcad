# SPDX-License-Identifier: MIT
# The STEP read is one of the geometry's operations (mitcad#2028): a crash
# inside OCCT's reader is the error "the STEP file could not be read",
# twice in a row (the exchange lock released), not a crash.
target_sources(test_geometry_base PRIVATE tests/test_step_read_crash.cpp)
target_link_libraries(test_geometry_base PRIVATE mitcad_geometry_io)
add_test(NAME geometry.step_read_crash
  COMMAND test_geometry_base step_read_crash "${CMAKE_CURRENT_BINARY_DIR}/step-read-crash")
set_tests_properties(geometry.step_read_crash PROPERTIES ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=step" TIMEOUT 120)
