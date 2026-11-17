# SPDX-License-Identifier: MIT
# A Web's Extend Ends, on for a new web, and its mitred corner
# (mitcad#1968): the walls reach the walls, one body of the analytic
# volume. The Linux test's shared input script
# (tools/ui-web-extend-ends-test.sh).
add_test(NAME app.ui-windows-web-extend-ends
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-web-extend-ends-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-web-extend-ends"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-web-extend-ends PROPERTIES TIMEOUT 300)
