# SPDX-License-Identifier: MIT
# Press Pull on a face continued (mitcad#1850): a bent tube's end opens
# Revolve1's edit, then goes Along the Walls with its ring dragged past the
# walls' inner radius; a T-slot bar's end Straight with a Scale taper. The
# shared steps of tools/ui-press-pull-continued-test.sh and
# tools/ui-windows-press-pull-continued-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-press-pull-continued
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-press-pull-continued-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-press-pull-continued")
  set_tests_properties(app.ui-macos-press-pull-continued PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
