# SPDX-License-Identifier: MIT
# A panel's new error scrolled into view by the panel itself (mitcad#1884):
# the shared steps of tools/ui-panel-message-reveal-test.sh in the Qt
# frontend, Docked and Floating, the error box and the field typed in within
# the page's view.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-panel-message-reveal
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-panel-message-reveal-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-panel-message-reveal")
  set_tests_properties(app.ui-macos-panel-message-reveal PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
