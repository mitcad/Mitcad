# SPDX-License-Identifier: MIT
# The sketch Point at the top level of the Create menu, after Lines,
# and as a button of its own in the ribbon's Create group (mitcad#1545,
# GitHub mitcad/Mitcad#200). The Linux test's shared input script
# (tools/ui-sketch-point-top-level-test.sh).
add_test(NAME app.ui-windows-sketch-point-top-level
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-point-top-level-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-point-top-level"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-point-top-level PROPERTIES TIMEOUT 180)
