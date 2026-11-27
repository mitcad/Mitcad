# SPDX-License-Identifier: MIT
# Lofted Flange (mitcad#2512): a rectangle and a circle above it picked as
# the profiles, OK, the press brake transition one body; the lofted flange
# edited loads its profiles. The Linux test's shared input script
# (tools/ui-sheet-lofted-flange-test.sh).
add_test(NAME app.ui-windows-sheet-lofted-flange
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-lofted-flange-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-lofted-flange"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-lofted-flange PROPERTIES TIMEOUT 240)
