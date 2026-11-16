# SPDX-License-Identifier: MIT
# The session log, the hang marker and the notice of an unexpected end
# (mitcad#1896): the shared steps of tools/ui-session-log-test.sh in the Qt
# frontend, then the files of the test's own log folder.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-session-log
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-session-log-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-session-log")
  set_tests_properties(app.ui-macos-session-log PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
