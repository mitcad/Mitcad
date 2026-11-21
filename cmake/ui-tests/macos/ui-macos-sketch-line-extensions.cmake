# SPDX-License-Identifier: MIT
# Resting the pointer on a sketch line acquires it for its extension
# (mitcad#2035): the steps tools/ui-scripts/sketch-line-extensions.mitcad-ui
# shares with the Linux test and app.ui-windows-sketch-line-extensions, in
# the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-line-extensions
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-line-extensions-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-line-extensions")
  set_tests_properties(app.ui-macos-sketch-line-extensions PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
