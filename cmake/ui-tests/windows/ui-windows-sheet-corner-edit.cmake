# SPDX-License-Identifier: MIT
# A saved Corner Round beside a bend edited from the timeline: the
# panel lists its corner and the view highlights it on the edge
# between the wall and the bend's side (mitcad#1766). The Linux test's
# shared input script (tools/ui-sheet-corner-edit-test.sh).
add_test(NAME app.ui-windows-sheet-corner-edit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-corner-edit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-corner-edit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-corner-edit PROPERTIES TIMEOUT 300)
