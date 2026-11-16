# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  foreach(cancel_case previews cancel quit)
    add_test(NAME app.ui-macos-command-cancel-${cancel_case}
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-command-cancel-test.sh"
              "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-command-cancel-${cancel_case}")
    set_tests_properties(app.ui-macos-command-cancel-${cancel_case} PROPERTIES
      TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
      ENVIRONMENT "MITCAD_TEST_CANCEL_CASE=${cancel_case}")
  endforeach()
endif()
