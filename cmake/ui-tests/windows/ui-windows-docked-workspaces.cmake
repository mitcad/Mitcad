# SPDX-License-Identifier: MIT
# The Docked layout (mitcad#2082): the Floating layout's top bar at the
# window's top and the toolbar panel under it, no menu bar row, the Projects
# panel's one choice; the workspace selector: Make's tabs alone,
# Render's tab, docks and rendered view and back, none of Render's docks in
# the panels' menu outside it, a drawing hiding the model's docks and Close
# Drawing showing them again (also after the Start page), the selector
# locked while sketching. The Linux test's shared input script
# (tools/ui-docked-workspaces-test.sh).
add_test(NAME app.ui-windows-docked-workspaces
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-workspaces-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-workspaces"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-docked-workspaces PROPERTIES TIMEOUT 300)
