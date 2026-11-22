# SPDX-License-Identifier: MIT
# The 3D view's Follow System background follows MitCad's theme
# (mitcad#2145): the shared steps of tools/ui-view-background-theme-test.sh
# in the Settings window of the Qt frontend: dark with the dark theme (by
# preference and by the appearance), light with the light one, at once when
# it changes, and again after Restore Defaults.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-view-background-theme
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-view-background-theme-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-view-background-theme")
  set_tests_properties(app.ui-macos-view-background-theme PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
