# SPDX-License-Identifier: MIT
# A Center to Center Slot's typed length and width become its
# dimensions, so that only its place is free (mitcad#1225); the Linux
# test's shared input script and mitcad-cli's check.
add_test(NAME app.ui-windows-sketch-slot-dimensions
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-slot-dimensions-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-slot-dimensions"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-slot-dimensions PROPERTIES TIMEOUT 240)
