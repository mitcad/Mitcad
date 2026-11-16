# SPDX-License-Identifier: MIT
# Flange's remembered choices (mitcad#1868): the shared steps of
# tools/ui-sheet-flange-remembered-test.sh in the Qt frontend, a new Flange
# starting with the last OK's Measured and Position and an edit loading its
# own.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-flange-remembered
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-flange-remembered-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-flange-remembered")
  set_tests_properties(app.ui-macos-sheet-flange-remembered PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
