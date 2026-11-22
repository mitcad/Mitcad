# SPDX-License-Identifier: MIT
# Horizontal/Vertical in the other direction (mitcad#2184): the steps
# tools/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui shares with
# the Linux test and app.ui-windows-sketch-horizontal-vertical-other, in
# the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-horizontal-vertical-other
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-horizontal-vertical-other-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-horizontal-vertical-other")
  set_tests_properties(app.ui-macos-sketch-horizontal-vertical-other PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
