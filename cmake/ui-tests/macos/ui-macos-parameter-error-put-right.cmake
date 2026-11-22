# SPDX-License-Identifier: MIT
# A parameter error put right leaves the message band (mitcad#2181): the
# steps tools/ui-scripts/parameter-error-put-right.mitcad-ui shares with the
# Linux test and app.ui-windows-parameter-error-put-right, in the Qt
# frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-parameter-error-put-right
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-parameter-error-put-right-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-parameter-error-put-right")
  set_tests_properties(app.ui-macos-parameter-error-put-right PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
