# SPDX-License-Identifier: MIT
# A 3D mouse (mitcad#951) and its Lock Rotation (mitcad#1910): the steps
# tools/ui-scripts/space-mouse.mitcad-ui shares with the Linux test and
# app.ui-windows-space-mouse, in the Qt frontend (IOKit HID finds no
# device in the VM; the test driver gives the input).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-space-mouse
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-space-mouse-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-space-mouse")
  set_tests_properties(app.ui-macos-space-mouse PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
