# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND Python3_Interpreter_FOUND
   AND TARGET mitcad-cli AND MITCAD_MACOS_NATIVE_UI AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cut-components
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cut-components-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cut-components")
  set_tests_properties(app.ui-macos-cut-components PROPERTIES TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
