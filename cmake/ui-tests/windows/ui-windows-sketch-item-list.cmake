# SPDX-License-Identifier: MIT
# Sketch mode's list of the sketch's work steps in the timeline's place
# (mitcad#1750, after mitcad#1492's list of items): a Project, a
# Rectangle and a dimension as three cells, the hover list, the error
# badge, Delete from a cell, and a sketch saved without steps. The
# Linux test's shared input script (tools/ui-sketch-item-list-test.sh),
# run in the Docked and the Floating layout (mitcad#1763).
add_test(NAME app.ui-windows-sketch-item-list
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-item-list-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-item-list"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-item-list PROPERTIES TIMEOUT 600)
