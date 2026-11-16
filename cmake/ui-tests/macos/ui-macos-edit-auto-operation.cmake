# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli AND Python3_Interpreter_FOUND)
  add_test(NAME app.ui-macos-edit-auto-operation
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-edit-auto-operation-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-edit-auto-operation")
  set_tests_properties(app.ui-macos-edit-auto-operation PROPERTIES
    TIMEOUT 960 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
