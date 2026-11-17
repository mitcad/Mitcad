# SPDX-License-Identifier: MIT
# The sketch Rectangular Pattern's Direction 2 (mitcad#1969): the steps
# tools/ui-scripts/sketch-pattern-honeycomb.mitcad-ui shares with the Linux
# test and app.ui-windows-sketch-pattern-honeycomb, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-pattern-honeycomb
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-pattern-honeycomb-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-pattern-honeycomb")
  set_tests_properties(app.ui-macos-sketch-pattern-honeycomb PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
