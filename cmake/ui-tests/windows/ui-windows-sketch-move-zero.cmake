# SPDX-License-Identifier: MIT
# Sketch Move/Copy starts at 0 (mitcad#1163): opened on a selected line
# it leaves the line where it is until a distance is typed; the Linux
# test's shared input script.
add_test(NAME app.ui-windows-sketch-move-zero
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-move-zero-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-move-zero"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-move-zero PROPERTIES TIMEOUT 300)
