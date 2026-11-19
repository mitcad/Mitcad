# SPDX-License-Identifier: MIT
# The command layout's user layer (mitcad#1951, mitcad#1991): a command
# moved and shown in two groups, a default button removed and a group moved,
# both kept after a restart, Show on Toolbar, the default restored, in the
# Floating and the Docked layout; the Linux test's shared input scripts
# (tools/ui-command-layout-test.sh).
add_test(NAME app.ui-windows-command-layout
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-command-layout-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-command-layout"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-command-layout PROPERTIES TIMEOUT 720)
