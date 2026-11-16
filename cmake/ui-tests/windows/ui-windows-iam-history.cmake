# SPDX-License-Identifier: MIT
# An .iam assembly's Parts with their design history (mitcad#507): the
# test project imported with and without, the parts' progress and
# outcomes, the volumes both ways, the choice remembered; with
# MITCAD_IAM_UI_ASSEMBLY or MITCAD_IPT_CORPUS the test files' seven-part
# assembly too (skipped without).
add_test(NAME app.ui-windows-iam-history
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-iam-history-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-iam-history"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-iam-history PROPERTIES TIMEOUT 3000)
