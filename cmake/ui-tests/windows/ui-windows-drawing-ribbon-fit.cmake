# SPDX-License-Identifier: MIT
# The DRAWING tab's groups at 1280 px (mitcad#2066): Dimensions, Text,
# Symbols and Tables stay whole in the Floating and the Docked layout, with
# labels and without. The Linux test's shared input script
# (tools/ui-drawing-ribbon-fit-test.sh).
add_test(NAME app.ui-windows-drawing-ribbon-fit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-ribbon-fit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-ribbon-fit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-ribbon-fit PROPERTIES TIMEOUT 360)
