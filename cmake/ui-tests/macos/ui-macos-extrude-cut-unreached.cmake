# SPDX-License-Identifier: MIT
# An Extrude Cut whose preview fails before its cut's targets are found
# (mitcad#2118): the shared steps of tools/ui-extrude-cut-unreached-test.sh
# in the Qt frontend, Docked and Floating: the Affected components line
# gives the failure in words and the panel's error is in the page's view.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-extrude-cut-unreached
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-extrude-cut-unreached-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-extrude-cut-unreached")
  set_tests_properties(app.ui-macos-extrude-cut-unreached PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
