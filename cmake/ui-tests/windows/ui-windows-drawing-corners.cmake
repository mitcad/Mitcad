# SPDX-License-Identifier: MIT
# Corners on drawings (mitcad#1338): a corner dimensioned to a hole's
# centre line in both pick orders, Always show corners' dots from the
# settings, the PDF and SVG the same with them on and off.
add_test(NAME app.ui-windows-drawing-corners
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-corners-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-corners"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-corners PROPERTIES TIMEOUT 300)
