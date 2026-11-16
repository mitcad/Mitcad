# SPDX-License-Identifier: MIT
# A sketch text moved after it is made (mitcad#1911): placed with the
# Text tool, moved with Move/Copy, dragged onto a point and placed by
# Edit Text's Position, its anchor read back in Edit Text each time. The
# Linux test's shared input script (tools/ui-sketch-move-text-test.sh).
add_test(NAME app.ui-windows-sketch-move-text
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-move-text-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-move-text"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-move-text PROPERTIES TIMEOUT 300)
