# SPDX-License-Identifier: MIT
# The tabs' menu (mitcad#1975): the shared steps of
# tools/ui-toolbar-tabs-test.sh in the Qt frontend, a tab hidden and shown
# again, Customize Toolbar… and Reset Toolbar, in both layouts.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-toolbar-tabs
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-toolbar-tabs-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-toolbar-tabs")
  set_tests_properties(app.ui-macos-toolbar-tabs PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
