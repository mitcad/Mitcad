# SPDX-License-Identifier: MIT
# The window's place and size (framework/WindowPlace) through the shared
# input scripts on macOS (Linux: tools/ui-window-place-test.sh, Windows:
# app.ui-windows-window-place).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-window-place
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-window-place-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-window-place")
  set_tests_properties(app.ui-macos-window-place PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
