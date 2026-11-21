# SPDX-License-Identifier: MIT
# The Hem command (mitcad#2112): the shared steps of
# tools/ui-sheet-hem-test.sh in the Qt frontend, a hem made, changed to
# Open, Rolled and Double, unfolded and refolded.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-hem
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-hem-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-hem")
  set_tests_properties(app.ui-macos-sheet-hem PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
