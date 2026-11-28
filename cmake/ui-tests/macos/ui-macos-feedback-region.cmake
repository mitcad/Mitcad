# SPDX-License-Identifier: MIT
# Send Feedback on a Region (mitcad#2616): the steps
# tools/ui-scripts/feedback-region.mitcad-ui shares with the Linux test and
# app.ui-windows-feedback-region, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-feedback-region
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-feedback-region-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-feedback-region")
  set_tests_properties(app.ui-macos-feedback-region PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
