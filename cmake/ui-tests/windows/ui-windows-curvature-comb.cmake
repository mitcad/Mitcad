# SPDX-License-Identifier: MIT
# Curvature Comb (mitcad#2520): Inspect > Curvature Comb on a cylinder's top
# edge, and sketch mode's Curvature comb on an S-shaped spline with its one
# inflection, read again while the spline's end point is dragged. The Linux
# test's shared input script (tools/ui-curvature-comb-test.sh).
add_test(NAME app.ui-windows-curvature-comb
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-curvature-comb-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-curvature-comb"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-curvature-comb PROPERTIES TIMEOUT 360)
