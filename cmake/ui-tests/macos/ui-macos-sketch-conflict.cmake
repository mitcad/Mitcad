# SPDX-License-Identifier: MIT
# Conflicting sketch constraints (mitcad#1566) and Replace for a constraint
# or dimension one item blocks, with Shift as a second way (mitcad#2090):
# the steps tools/ui-scripts/sketch-conflict.mitcad-ui and
# tools/ui-scripts/sketch-conflict-replace.mitcad-ui share with the Linux
# test and app.ui-windows-sketch-conflict, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-conflict
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-conflict-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-conflict")
  set_tests_properties(app.ui-macos-sketch-conflict PROPERTIES
    TIMEOUT 720 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
