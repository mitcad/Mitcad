# SPDX-License-Identifier: MIT
# Sketch patterns (mitcad#2007): the steps
# tools/ui-scripts/sketch-pattern-free.mitcad-ui shares with the Linux test
# and app.ui-windows-sketch-pattern-free, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-pattern-free
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-pattern-free-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-pattern-free")
  set_tests_properties(app.ui-macos-sketch-pattern-free PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
