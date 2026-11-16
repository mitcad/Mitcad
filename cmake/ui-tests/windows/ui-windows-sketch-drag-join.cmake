# SPDX-License-Identifier: MIT
# A dragged sketch point snaps to points and curves and is held where
# it is dropped, in the drag's undo step; not with Shift (mitcad#1538).
# The Linux test's shared input script (tools/ui-sketch-drag-join-test.sh).
add_test(NAME app.ui-windows-sketch-drag-join
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-drag-join-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-drag-join"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-drag-join PROPERTIES TIMEOUT 240)
