# SPDX-License-Identifier: MIT
# The view follows a sketch its first dimension scales (mitcad#1948): the
# steps tools/ui-scripts/sketch-scale-view.mitcad-ui shares with the Linux
# test and app.ui-windows-sketch-scale-view, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-scale-view
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-scale-view-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-scale-view")
  set_tests_properties(app.ui-macos-sketch-scale-view PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
