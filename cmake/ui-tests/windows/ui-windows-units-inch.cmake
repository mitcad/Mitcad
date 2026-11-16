# SPDX-License-Identifier: MIT
# Lengths in the design's unit (mitcad#1165), the steps of
# tools/ui-units-inch-test.sh: in a design in inches the sheet-rules
# row, Extrude's default distance and Measure in inches.
add_test(NAME app.ui-windows-units-inch
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-units-inch-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-units-inch"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-units-inch PROPERTIES TIMEOUT 180)
