# SPDX-License-Identifier: MIT
# Timber's physical fastener seating through the retained Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  find_program(MITCAD_TIMBER_TEST_GIT git)
  if(Python3_Interpreter_FOUND AND MITCAD_TIMBER_TEST_GIT)
    add_test(NAME app.ui-macos-timber-seating
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-timber-seating-test.py"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-timber-seating")
    set_tests_properties(app.ui-macos-timber-seating PROPERTIES
      TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
  endif()
endif()
