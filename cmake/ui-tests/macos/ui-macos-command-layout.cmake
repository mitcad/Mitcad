# SPDX-License-Identifier: MIT
# The command layout's user layer (mitcad#1951, mitcad#1991): the shared
# steps of tools/ui-command-layout-test.sh in the Qt frontend, a command
# moved and shown in two groups, a default button removed and a group moved,
# both kept after a restart, Show on Toolbar from the group caption, the
# default restored, in both layouts.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-command-layout
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-command-layout-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-command-layout")
  set_tests_properties(app.ui-macos-command-layout PROPERTIES
    TIMEOUT 840 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
