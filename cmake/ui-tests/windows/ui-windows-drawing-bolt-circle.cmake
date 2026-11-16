# SPDX-License-Identifier: MIT
# A flange's bolt circle and the angle between its holes (mitcad#1693):
# the Dimension tool's Auto on a centre pattern's circle, Ø60 B.C.; its
# Angle between two radial lines of the pattern, 60°.
add_test(NAME app.ui-windows-drawing-bolt-circle
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-bolt-circle-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-bolt-circle"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-bolt-circle PROPERTIES TIMEOUT 300)
