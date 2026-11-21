# SPDX-License-Identifier: MIT
# Sketch mode's list of the sketch's work steps in the timeline's place
# (mitcad#1750, mitcad#2089): the steps
# tools/ui-scripts/sketch-item-list.mitcad-ui shares with the Linux test and
# app.ui-windows-sketch-item-list, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-item-list
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-item-list-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-item-list")
  set_tests_properties(app.ui-macos-sketch-item-list PROPERTIES
    TIMEOUT 720 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
