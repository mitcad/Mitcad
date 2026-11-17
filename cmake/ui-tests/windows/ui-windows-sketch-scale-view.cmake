# SPDX-License-Identifier: MIT
# The view follows a sketch its first dimension scales (mitcad#1948): a
# diameter that scales the sketch up and one that scales it down, the
# dimensioned circle's centre kept at its place in the window. The Linux
# test's shared input script (tools/ui-sketch-scale-view-test.sh).
add_test(NAME app.ui-windows-sketch-scale-view
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-scale-view-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-scale-view"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-scale-view PROPERTIES TIMEOUT 180)
