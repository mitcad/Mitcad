# SPDX-License-Identifier: MIT
# Sketch Scale (mitcad#2109): a circle and a line scaled by 1.5 about the
# center of the circle with their dimensions, undone, then scaled copies
# with Keep Values and Create Copy. The Linux test's shared input script
# (tools/ui-sketch-scale-test.sh).
add_test(NAME app.ui-windows-sketch-scale
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-scale-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-scale"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-scale PROPERTIES TIMEOUT 240)
