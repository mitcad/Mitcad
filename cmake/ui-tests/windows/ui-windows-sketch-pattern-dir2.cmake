# SPDX-License-Identifier: MIT
# The sketch Rectangular Pattern's second direction (mitcad#1640): a
# 3 x 2 grid by default, an arrow for each direction whose drag sets
# its spacing, a faint one at Quantity 2 of 1. The Linux test's shared
# input script (tools/ui-sketch-pattern-dir2-test.sh).
add_test(NAME app.ui-windows-sketch-pattern-dir2
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-pattern-dir2-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-pattern-dir2"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-pattern-dir2 PROPERTIES TIMEOUT 300)
