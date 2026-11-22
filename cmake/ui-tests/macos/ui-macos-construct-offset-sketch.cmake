# SPDX-License-Identifier: MIT
# Offset Plane takes a sketch (mitcad#2167): the sketch picked in the
# Browser and its profile clicked in the view are each taken as the
# sketch's plane, OK makes the plane; the shared steps of
# tools/ui-construct-offset-sketch-test.sh and
# tools/ui-windows-construct-offset-sketch-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-construct-offset-sketch
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-construct-offset-sketch-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-construct-offset-sketch")
  set_tests_properties(app.ui-macos-construct-offset-sketch PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
