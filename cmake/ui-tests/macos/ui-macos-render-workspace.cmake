# SPDX-License-Identifier: MIT
# The Render workspace (mitcad#2013): the shared steps of
# tools/ui-render-workspace-test.sh in the Qt frontend, its stages, the
# docked environment and lights, shots and Render All Shots (skipped while
# the macOS window has no workspace selector, mitcad#1990).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-render-workspace
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-workspace-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-render-workspace")
  set_tests_properties(app.ui-macos-render-workspace PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
