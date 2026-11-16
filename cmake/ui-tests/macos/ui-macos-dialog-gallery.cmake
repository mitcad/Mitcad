# SPDX-License-Identifier: MIT
# Library/part controls and the shared Qt dialog conventions (mitcad#645).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE AND Python3_Interpreter_FOUND)
  add_test(NAME app.ui-macos-dialog-gallery
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dialog-gallery-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-dialog-gallery")
  set_tests_properties(app.ui-macos-dialog-gallery PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
