# SPDX-License-Identifier: MIT
# Trim's drag (mitcad#2524): the steps
# tools/ui-scripts/sketch-trim-drag.mitcad-ui shares with the Linux test
# and app.ui-windows-sketch-trim-drag, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-trim-drag
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-trim-drag-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-trim-drag")
  set_tests_properties(app.ui-macos-sketch-trim-drag PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
