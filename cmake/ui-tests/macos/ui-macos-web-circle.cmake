# SPDX-License-Identifier: MIT
# A Web from a circle (mitcad#2364): the shared steps of
# tools/ui-web-circle-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-web-circle
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-web-circle-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-web-circle")
  set_tests_properties(app.ui-macos-web-circle PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
