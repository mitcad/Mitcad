# SPDX-License-Identifier: MIT
# A section that shows a body whole because its clip failed is reported
# in the message band and the Analysis folder's row, withdrawn when the
# analysis is hidden; a plane that cuts nothing is told (mitcad#1230);
# the Linux test's shared input script.
add_test(NAME app.ui-windows-section-clip-report
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-clip-report-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-section-clip-report"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-section-clip-report PROPERTIES TIMEOUT 240)
