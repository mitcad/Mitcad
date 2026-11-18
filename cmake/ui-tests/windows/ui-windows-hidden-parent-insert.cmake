# SPDX-License-Identifier: MIT
# Inserting into a component under a hidden parent (mitcad#1995): the
# rows in a hidden occurrence have the dimmed closed eye; an insert and New
# Component in it show the parent in their undo step and say so in the
# status bar. The Linux test's shared input script
# (tools/ui-hidden-parent-insert-test.sh).
add_test(NAME app.ui-windows-hidden-parent-insert
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hidden-parent-insert-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-hidden-parent-insert"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-hidden-parent-insert PROPERTIES TIMEOUT 240)
