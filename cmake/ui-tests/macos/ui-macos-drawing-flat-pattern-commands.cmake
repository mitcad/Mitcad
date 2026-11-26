# SPDX-License-Identifier: MIT
# The Flat Pattern's drawing commands (mitcad#2446): Flat Pattern View from
# the DRAWING tab's Create menu and Create Drawing of Flat Pattern from the
# Flat Pattern row each place the blank with its bend line: the shared steps
# of tools/ui-drawing-flat-pattern-commands-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-drawing-flat-pattern-commands
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drawing-flat-pattern-commands-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-drawing-flat-pattern-commands")
  set_tests_properties(app.ui-macos-drawing-flat-pattern-commands PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
