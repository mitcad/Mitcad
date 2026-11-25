# SPDX-License-Identifier: MIT
# A Web from a circle (mitcad#2364): a ring wall joined to the plate, one
# body of the analytic volume. The Linux test's shared input script
# (tools/ui-web-circle-test.sh).
add_test(NAME app.ui-windows-web-circle
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-web-circle-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-web-circle"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-web-circle PROPERTIES TIMEOUT 300)
