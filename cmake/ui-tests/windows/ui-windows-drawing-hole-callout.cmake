# SPDX-License-Identifier: MIT
# A drawing's own hole callout (mitcad#2054): Hole Callout on a
# counterbored hole, Text After in its panel, the hole followed. The Linux
# test's shared input script (tools/ui-drawing-hole-callout-test.sh).
add_test(NAME app.ui-windows-drawing-hole-callout
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-hole-callout-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-hole-callout"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-hole-callout PROPERTIES TIMEOUT 240)
