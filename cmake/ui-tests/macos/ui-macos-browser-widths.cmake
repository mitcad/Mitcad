# SPDX-License-Identifier: MIT
# The Browser's width in the Floating layout (mitcad#1932): the shared steps
# of tools/ui-browser-widths-test.sh in the Qt frontend, Rows widening to its
# names up to a share of the view and Card's width grip, remembered over a
# restart.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-browser-widths
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-browser-widths-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-browser-widths")
  set_tests_properties(app.ui-macos-browser-widths PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
