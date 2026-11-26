# SPDX-License-Identifier: MIT
# The Flat Pattern's drawing commands (mitcad#2446): Flat Pattern View from
# the DRAWING tab's Create menu and Create Drawing of Flat Pattern from the
# Flat Pattern row each place the blank with its bend line. The Linux test's
# shared input script (tools/ui-drawing-flat-pattern-commands-test.sh).
add_test(NAME app.ui-windows-drawing-flat-pattern-commands
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-flat-pattern-commands-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-flat-pattern-commands"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-flat-pattern-commands PROPERTIES TIMEOUT 240)
