# SPDX-License-Identifier: MIT
# Convert to Sheet Metal (mitcad#1949): the shared steps of
# tools/ui-sheet-convert-test.sh in the Qt frontend, a solid tray's base
# and its four bottom edges picked, OK, one sheet with its corners ripped.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-convert
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-convert-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-convert")
  set_tests_properties(app.ui-macos-sheet-convert PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
