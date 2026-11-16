# SPDX-License-Identifier: MIT
# The values of two concentric circles' diameters, dimensioned in turn,
# keep apart after the fit, zoomed in and zoomed out (mitcad#1673). The
# Linux test's shared input script
# (tools/ui-sketch-concentric-dimensions-test.sh).
add_test(NAME app.ui-windows-sketch-concentric-dimensions
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-concentric-dimensions-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-concentric-dimensions"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-concentric-dimensions PROPERTIES TIMEOUT 300)
