# SPDX-License-Identifier: MIT
# Auto Constrain (mitcad#2019): the steps
# tools/ui-scripts/sketch-auto-constrain.mitcad-ui shares with the Linux
# test and app.ui-windows-sketch-auto-constrain, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-auto-constrain
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-auto-constrain-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-auto-constrain")
  set_tests_properties(app.ui-macos-sketch-auto-constrain PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
