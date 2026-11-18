# SPDX-License-Identifier: MIT
# Finish Sketch after the window was resized in the sketch restores the
# view in the window's aspect, not squashed (mitcad#1996). The Linux
# test's shared input script (tools/ui-sketch-resize-test.sh).
add_test(NAME app.ui-windows-sketch-resize
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-resize-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-resize"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-resize PROPERTIES TIMEOUT 300)
