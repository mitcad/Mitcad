# SPDX-License-Identifier: MIT
# Plates… (mitcad#1050), the Linux test's shared input script: two
# plates added, a body picked onto Plate 2 in vase mode, the plate
# drawn beside the design; Send to Slicer writes one 3MF of both
# plates, the spiral mode on Plate 2 only.
add_test(NAME app.ui-windows-print-plates
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-plates-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-print-plates"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-print-plates PROPERTIES TIMEOUT 420)
