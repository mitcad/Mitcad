# SPDX-License-Identifier: MIT
# The Browser's component color bars (mitcad#1876): the shared steps of
# tools/ui-browser-component-colors-test.sh in the Qt frontend, every
# component, occurrence and body row in its component's timeline color.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-browser-component-colors
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-browser-component-colors-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-browser-component-colors")
  set_tests_properties(app.ui-macos-browser-component-colors PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
