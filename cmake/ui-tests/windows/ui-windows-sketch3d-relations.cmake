# SPDX-License-Identifier: MIT
# 3D relations from the 3D Sketch Palette (mitcad#1661): Parallel,
# Perpendicular and Angle of lines in space, Symmetric of two points
# about the sketch's YZ plane. The Linux test's shared input script
# (tools/ui-sketch3d-relations-test.sh).
add_test(NAME app.ui-windows-sketch3d-relations
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch3d-relations-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch3d-relations"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch3d-relations PROPERTIES TIMEOUT 300)
