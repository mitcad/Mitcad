# SPDX-License-Identifier: MIT
# OCCT's crash handlers next to the application's crash report (mitcad#62).
if(NOT WIN32)
  target_sources(test_geometry_base PRIVATE tests/test_guard.cpp)
  add_test(NAME geometry.guard_inside COMMAND test_geometry_base guard inside)
  set_tests_properties(geometry.guard_inside PROPERTIES PASS_REGULAR_EXPRESSION "converted: SIGSEGV")
  add_test(NAME geometry.guard_outside COMMAND test_geometry_base guard outside)
  set_tests_properties(geometry.guard_outside PROPERTIES PASS_REGULAR_EXPRESSION "earlier handler: signal 11")
  # The wrapper of the analysis bridge's operations (mitcad#392).
  add_test(NAME geometry.guard_operation COMMAND test_geometry_base guard operation)
  set_tests_properties(geometry.guard_operation PROPERTIES ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=clip"
    PASS_REGULAR_EXPRESSION "error: clip: SIGSEGV")
endif()
