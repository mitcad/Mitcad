# SPDX-License-Identifier: MIT
# The Docked toolbar's colours in the dark and the light theme: the
# Classic ribbon's body and the bar's Undo, Redo, separator and Chat
# buttons on the toolbar's surface (GitHub #127, mitcad#1245); the Linux
# test's shared input script.
add_test(NAME app.ui-windows-docked-toolbar-colors
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-toolbar-colors-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-toolbar-colors"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-docked-toolbar-colors PROPERTIES TIMEOUT 180)
