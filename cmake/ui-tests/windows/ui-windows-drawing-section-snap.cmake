# SPDX-License-Identifier: MIT
# A section's line and a break's sides snap (mitcad#1362): onto a hole's
# centre and its centre line, a second click turned to 45 degrees,
# Shift as clicked, a break's sides onto points.
add_test(NAME app.ui-windows-drawing-section-snap
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-section-snap-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-section-snap"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-section-snap PROPERTIES TIMEOUT 300)
