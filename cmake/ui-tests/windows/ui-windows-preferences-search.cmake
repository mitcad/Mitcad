# SPDX-License-Identifier: MIT
# Preferences' search (mitcad#2074, GitHub mitcad/Mitcad#450): the counts
# in the page list, the greyed pages, the highlighted settings, Return, Esc,
# a sub-tab and the command search's way in. The Linux test's shared input
# script (tools/ui-preferences-search-test.sh).
add_test(NAME app.ui-windows-preferences-search
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preferences-search-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-preferences-search"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-preferences-search PROPERTIES TIMEOUT 300)
