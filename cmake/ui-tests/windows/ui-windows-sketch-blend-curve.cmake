# SPDX-License-Identifier: MIT
# Blend Curve (mitcad#2107): two lines blended with Tangent and two arcs
# with Curvature, held by their constraints through a new dimension. The
# Linux test's shared input script (tools/ui-sketch-blend-curve-test.sh).
add_test(NAME app.ui-windows-sketch-blend-curve
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-blend-curve-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-blend-curve"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-blend-curve PROPERTIES TIMEOUT 300)
