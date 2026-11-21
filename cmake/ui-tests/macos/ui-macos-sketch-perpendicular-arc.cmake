# SPDX-License-Identifier: MIT
# Perpendicular of a line and an arc (mitcad#2114): the steps
# tools/ui-scripts/sketch-perpendicular-arc.mitcad-ui shares with the Linux
# test and app.ui-windows-sketch-perpendicular-arc, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-perpendicular-arc
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-perpendicular-arc-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-perpendicular-arc")
  set_tests_properties(app.ui-macos-sketch-perpendicular-arc PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
