# SPDX-License-Identifier: MIT
# The sketch's x and y axes as the sketch Rectangular Pattern's
# Direction (mitcad#1655): in a sketch of a circle and no line, the y
# axis picked runs the copies along y, the x axis along x. The Linux
# test's shared input script (tools/ui-sketch-pattern-axis-test.sh).
add_test(NAME app.ui-windows-sketch-pattern-axis
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-pattern-axis-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-pattern-axis"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-pattern-axis PROPERTIES TIMEOUT 300)
