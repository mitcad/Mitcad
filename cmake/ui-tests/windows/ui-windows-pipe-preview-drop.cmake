# SPDX-License-Identifier: MIT
# A value typed while a slow preview computes drops that preview
# (mitcad#1788): Pipe's default size held by the test's worker gate,
# the size 2 typed, the 2 mm pipe previewed and made. The Linux test's
# shared input script (tools/ui-pipe-preview-drop-test.sh).
add_test(NAME app.ui-windows-pipe-preview-drop
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-pipe-preview-drop-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-pipe-preview-drop"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-pipe-preview-drop PROPERTIES TIMEOUT 240)
