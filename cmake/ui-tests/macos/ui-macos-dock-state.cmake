# SPDX-License-Identifier: MIT
# The Docked layout's docks kept between starts (mitcad#2070, GitHub
# mitcad/Mitcad#451) through the shared input scripts on macOS (Linux:
# tools/ui-dock-state-test.sh, Windows: app.ui-windows-dock-state).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-dock-state
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-dock-state-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-dock-state")
  set_tests_properties(app.ui-macos-dock-state PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
