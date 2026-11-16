# SPDX-License-Identifier: MIT
# The Import Report's status on every row and its verdict (mitcad#1893):
# test_exchange's part with a design opened, everything in and every row
# OK; its test assembly with a missing file, that file's row an error.
# The Linux test's shared input scripts
# (tools/ui-import-report-status-test.sh).
if(TARGET test_exchange)
  add_test(NAME app.ui-windows-import-report-status
    COMMAND powershell -NoProfile -ExecutionPolicy Bypass
            -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-import-report-status-test.ps1"
            -App "$<TARGET_FILE:mitcad>" -TestExchange "$<TARGET_FILE:test_exchange>"
            -Out "${CMAKE_BINARY_DIR}/ui-windows-import-report-status"
            -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
  set_tests_properties(app.ui-windows-import-report-status PROPERTIES TIMEOUT 1200)
endif()
