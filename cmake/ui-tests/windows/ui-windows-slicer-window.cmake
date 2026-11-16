# SPDX-License-Identifier: MIT
# Send to Slicer's Use the open slicer window (mitcad#1162): with fake
# slicers, two sends of a design start one window and hand it the
# second file; another design starts its own; the box off starts a
# window per send; a program without a hand-over is brought forward.
add_test(NAME app.ui-windows-slicer-window
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-slicer-window-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-slicer-window"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-slicer-window PROPERTIES TIMEOUT 300)
