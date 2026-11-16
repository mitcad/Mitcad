# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-release AND Python3_Interpreter_FOUND)
  add_test(NAME app.ui-macos-update
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-update-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-release>" "${CMAKE_BINARY_DIR}/ui-macos-update")
  # A build without the update test hooks (mitcad#921) is checked to
  # ignore the tests' variables instead of updating.
  if(MITCAD_UPDATE_TEST_HOOKS)
    set(mitcad_update_test_hooks ON)
  else()
    set(mitcad_update_test_hooks OFF)
  endif()
  set_tests_properties(app.ui-macos-update PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
    ENVIRONMENT "UI_UPDATE_TEST_HOOKS=${mitcad_update_test_hooks}")
endif()
