# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-drawing-dimensions
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-drawing-dimensions-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-drawing-dimensions")
  set_tests_properties(app.ui-macos-sketch-drawing-dimensions PROPERTIES
    TIMEOUT 540 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
