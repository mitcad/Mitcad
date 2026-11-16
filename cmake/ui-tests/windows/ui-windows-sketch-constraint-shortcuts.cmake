# SPDX-License-Identifier: MIT
# The sketch constraints' Shift keys (mitcad#1229): Shift+P adds
# Parallel to two selected lines, Shift+V makes a line vertical, and
# Shift+P in a dimension's value field types P; the Linux test's shared
# input script.
add_test(NAME app.ui-windows-sketch-constraint-shortcuts
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-constraint-shortcuts-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-constraint-shortcuts"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-constraint-shortcuts PROPERTIES TIMEOUT 240)
