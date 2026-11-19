# SPDX-License-Identifier: MIT
# The tabs' menu (mitcad#1975): a tab hidden and shown again, Customize
# Toolbar... and Reset Toolbar, in the Floating and the Docked layout; the
# Linux test's shared input script (tools/ui-toolbar-tabs-test.sh).
add_test(NAME app.ui-windows-toolbar-tabs
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-toolbar-tabs-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-toolbar-tabs"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-toolbar-tabs PROPERTIES TIMEOUT 360)
