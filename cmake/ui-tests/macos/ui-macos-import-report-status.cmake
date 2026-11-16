# SPDX-License-Identifier: MIT
# The Import Report's status on every row and its verdict (mitcad#1893): the
# shared steps of tools/ui-import-report-status-test.sh in the Qt frontend,
# test_exchange's part with a design (everything in, every row OK) and its
# test assembly with a missing file (that file's row an error).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET test_exchange AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-import-report-status
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-import-report-status-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:test_exchange>" "${CMAKE_BINARY_DIR}/ui-macos-import-report-status")
  set_tests_properties(app.ui-macos-import-report-status PROPERTIES
    TIMEOUT 1200 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
