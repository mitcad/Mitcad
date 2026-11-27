# SPDX-License-Identifier: MIT
# Press Pull's Offset on a curved face (mitcad#2509): on a tapered
# Extrude's side the arrow starts on the face along its outward normal;
# dragged and typed, the same offset; 0.5 mm typed, the grown frustum. The
# shared steps of tools/ui-press-pull-curved-offset-test.sh and
# tools/ui-windows-press-pull-curved-offset-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-press-pull-curved-offset
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-press-pull-curved-offset-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-press-pull-curved-offset")
  set_tests_properties(app.ui-macos-press-pull-curved-offset PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
