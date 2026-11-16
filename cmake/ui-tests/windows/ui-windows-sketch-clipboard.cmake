# SPDX-License-Identifier: MIT
# Copy, Cut and Paste of sketch geometry (mitcad#788): a line cut from
# one sketch with its constraints, pasted into another by a click and
# by typed X and Y, undo and Esc.
add_test(NAME app.ui-windows-sketch-clipboard
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-clipboard-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-clipboard"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
# Cut, then Paste seconds later: another test writing the shared
# system clipboard in between would replace the fragment (see
# app.ui-windows-measure).
set_tests_properties(app.ui-windows-sketch-clipboard PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)
