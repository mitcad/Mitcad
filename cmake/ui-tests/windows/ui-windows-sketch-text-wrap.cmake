# SPDX-License-Identifier: MIT
# A sketch text drawn in a frame wraps to the frame's width (mitcad#2029):
# a frame dragged with the Text tool and a long text typed, Edit Text's
# Height, a frame corner, Wrap in Frame and a line break in its Text box.
# The Linux test's shared input script (tools/ui-sketch-text-wrap-test.sh).
add_test(NAME app.ui-windows-sketch-text-wrap
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-text-wrap-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-text-wrap"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-text-wrap PROPERTIES TIMEOUT 300)
