# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-perforate
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-perforate-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-perforate")
  set_tests_properties(app.ui-macos-sheet-perforate PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
