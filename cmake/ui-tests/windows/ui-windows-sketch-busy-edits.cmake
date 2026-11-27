# SPDX-License-Identifier: MIT
# Sketch edits asked for while a job computes the model (mitcad#2500,
# GitHub mitcad/Mitcad#706): Make Driven from a dimension's context menu
# and Delete wait for a job that holds the model, then each is done in one
# undo step. The Linux test's shared input script
# (tools/ui-sketch-busy-edits-test.sh).
add_test(NAME app.ui-windows-sketch-busy-edits
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-busy-edits-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-busy-edits"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-busy-edits PROPERTIES TIMEOUT 180)
