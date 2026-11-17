# SPDX-License-Identifier: MIT
# The sketch Rectangular Pattern's Direction 2 (mitcad#1969): a line at
# 60 degrees picked runs the second row of copies along it, and Edit
# Pattern keeps it. The Linux test's shared input script
# (tools/ui-sketch-pattern-honeycomb-test.sh).
add_test(NAME app.ui-windows-sketch-pattern-honeycomb
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-pattern-honeycomb-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-pattern-honeycomb"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-pattern-honeycomb PROPERTIES TIMEOUT 300)
