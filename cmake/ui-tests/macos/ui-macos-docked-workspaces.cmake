# SPDX-License-Identifier: MIT
# The Docked layout's workspace selector on macOS (mitcad#2082), where the
# system shows the menus and the toolbar has no top bar above it: the steps
# of tools/ui-scripts/docked-workspaces-macos.mitcad-ui in the Qt frontend
# (Windows and Linux, with the top bar: app.ui-windows-docked-workspaces).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-docked-workspaces
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-docked-workspaces-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-docked-workspaces")
  set_tests_properties(app.ui-macos-docked-workspaces PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
