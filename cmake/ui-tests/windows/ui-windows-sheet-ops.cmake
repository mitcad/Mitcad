# SPDX-License-Identifier: MIT
# Sheet-metal operations (mitcad#125): Fold, Bend, Corner Round and
# Chamfer, Corner Relief, Sheet Cut, Unfold and Refold, each with its
# picks, preview, a timeline edit and Undo.
add_test(NAME app.ui-windows-sheet-ops
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-ops-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-ops"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-ops PROPERTIES TIMEOUT 600)
