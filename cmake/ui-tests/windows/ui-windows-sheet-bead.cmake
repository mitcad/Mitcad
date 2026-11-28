# SPDX-License-Identifier: MIT
# The Bead command (mitcad#2645): a round bead pressed along a sketch line,
# changed to a trapezoid and pressed into the face by Edit Feature. The
# Linux test's shared input script (tools/ui-sheet-bead-test.sh).
add_test(NAME app.ui-windows-sheet-bead
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-bead-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-bead"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-bead PROPERTIES TIMEOUT 300)
