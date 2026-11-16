# SPDX-License-Identifier: MIT
# Send to Slicer's Process row shows the process set on the body
# (mitcad#1799): Per body for two bodies of different processes, a
# choice set on both bodies in one undo step, the 3MF read back. The
# Linux test's shared input script (tools/ui-body-process-send-test.sh).
add_test(NAME app.ui-windows-body-process-send
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-body-process-send-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-body-process-send"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-body-process-send PROPERTIES TIMEOUT 300)
