# SPDX-License-Identifier: MIT
# Closed fitted splines and tangents at a chosen end (mitcad#783): a
# spline tangent to two lines at its two ends, each picked near its
# end; a fit point spline closed by a click on its first point and
# extruded as one profile; the saved file has both.
add_test(NAME app.ui-windows-sketch-spline
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-spline-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-spline" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-spline PROPERTIES TIMEOUT 300)
