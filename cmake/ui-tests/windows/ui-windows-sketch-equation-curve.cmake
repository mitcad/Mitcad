# SPDX-License-Identifier: MIT
# Equation Curve (mitcad#2237): an involute of a parameter made, its profile
# extruded, the parameter changed and followed, the formula edited. The
# Linux test's shared input script (tools/ui-sketch-equation-curve-test.sh).
add_test(NAME app.ui-windows-sketch-equation-curve
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-equation-curve-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-equation-curve"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-equation-curve PROPERTIES TIMEOUT 360)
