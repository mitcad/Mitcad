# SPDX-License-Identifier: MIT
# The top bar's one left group (mitcad#2129, the quick-access group of
# mitcad#1090) in the Docked and the Floating layout: the default items in
# order after the fixed ☰ and Home, File removed and added back, Add to
# Quick Access at the group's end, Hide Quick-Access Toolbar hiding only the
# added commands, Print only with a drawing, a removed item kept removed
# after a restart, an earlier version's choice migrated once, the group
# folded with two designs in session 0's window.
add_test(NAME app.ui-windows-quick-access
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-quick-access-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-quick-access" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-quick-access PROPERTIES TIMEOUT 600)
