# SPDX-License-Identifier: MIT
# Offset Plane's Reverse Normal and signed Distance (mitcad#2048): the
# preview's normal marker turned over, a negative distance on the other
# side of the face; the shared steps of
# tools/ui-construct-reverse-normal-test.sh and
# tools/ui-windows-construct-reverse-normal-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-construct-reverse-normal
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-construct-reverse-normal-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-construct-reverse-normal")
  set_tests_properties(app.ui-macos-construct-reverse-normal PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
