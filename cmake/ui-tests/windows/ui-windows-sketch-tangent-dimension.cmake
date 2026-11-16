# SPDX-License-Identifier: MIT
# Sketch Dimension's right-click menu (mitcad#1540): Center or Tangent
# for a circle and Driving or Driven, a tangent-to-center and a driven
# tangent-to-tangent dimension between two circles. The Linux test's
# shared input script (tools/ui-sketch-tangent-dimension-test.sh).
add_test(NAME app.ui-windows-sketch-tangent-dimension
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-tangent-dimension-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-tangent-dimension"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-tangent-dimension PROPERTIES TIMEOUT 180)
