# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli AND Python3_Interpreter_FOUND AND MITCAD_MACOS_NATIVE_UI)
  add_test(NAME app.ui-macos-native-edit-auto-operation
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-edit-auto-operation-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-native-edit-auto-operation" native)
  set_tests_properties(app.ui-macos-native-edit-auto-operation PROPERTIES
    TIMEOUT 1920 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
