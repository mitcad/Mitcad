# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli)
  # Finish Sketch after the window was resized in the sketch restores the
  # view in the window's aspect, not squashed (mitcad#1996): the driver
  # script shared with the Linux and Windows tests, Qt frontend.
  add_test(NAME app.ui-macos-sketch-resize
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-resize-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-resize")
  set_tests_properties(app.ui-macos-sketch-resize PROPERTIES TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
