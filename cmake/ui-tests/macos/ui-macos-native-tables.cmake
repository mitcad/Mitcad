# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND MITCAD_MACOS_NATIVE_UI AND TARGET mitcad-cli AND Python3_Interpreter_FOUND)
  # Table cell semantics, cell edit keys and column sizing (mitcad#814),
  # with a local synthetic library, in both appearances.
  add_test(NAME app.ui-macos-native-tables
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-tables-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-tables")
  set_tests_properties(app.ui-macos-native-tables PROPERTIES TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
