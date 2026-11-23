# SPDX-License-Identifier: MIT
# The Browser rows' Activate radio button and Show eye places (mitcad#2223):
# the shared steps of tools/ui-browser-control-places-test.sh in the Qt
# frontend, Classic by default and on Right one column at the visible right
# edge in the Floating layout (Rows and Card) and the Docked one.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-browser-control-places
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-browser-control-places-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-browser-control-places")
  set_tests_properties(app.ui-macos-browser-control-places PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
