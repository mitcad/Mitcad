# SPDX-License-Identifier: MIT
# File > Export of a 3MF file with the design's print plates
# (mitcad#1679): all plates in one file, a file per plate, one plate.
# The Linux test's shared input script (tools/ui-export-plates-test.sh).
add_test(NAME app.ui-windows-export-plates
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-export-plates-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-export-plates"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-export-plates PROPERTIES TIMEOUT 300)
