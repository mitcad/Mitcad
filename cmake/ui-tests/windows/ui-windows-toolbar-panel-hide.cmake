# SPDX-License-Identifier: MIT
# View > Toolbar Panel (mitcad#2426, GitHub mitcad/Mitcad#571) in the Floating
# and the Docked layout: Ctrl+F1 hides the toolbar panel, a rest at the view's
# top edge reveals it, a command from it runs, it goes once the pointer left
# it, a drag across the edge shows nothing, the state kept after a restart.
# The Linux test's shared input scripts (tools/ui-toolbar-panel-hide-test.sh).
add_test(NAME app.ui-windows-toolbar-panel-hide
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-toolbar-panel-hide-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-toolbar-panel-hide"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-toolbar-panel-hide PROPERTIES TIMEOUT 600)
