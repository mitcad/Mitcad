# SPDX-License-Identifier: MIT
# Export Turntable (mitcad#2523): the shared steps of
# tools/ui-turntable-test.sh in the Qt frontend: 36 frames of a full turn
# stepping 10 degrees, and with FFmpeg the MP4 video's frame count and size.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-turntable
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-turntable-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-turntable")
  set_tests_properties(app.ui-macos-turntable PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
