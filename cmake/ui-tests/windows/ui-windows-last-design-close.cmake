# SPDX-License-Identifier: MIT
# Closing the last design (mitcad#2142): with Preferences > General > Quit
# MitCad when the last design is closed off (the default) File > Close and
# the only design's tab close button show the Start page and MitCad runs
# on; with it on (kept over a restart) they quit MitCad. The Linux test's
# shared input scripts (tools/ui-last-design-close-test.sh).
add_test(NAME app.ui-windows-last-design-close
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-last-design-close-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-last-design-close"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-last-design-close PROPERTIES TIMEOUT 600)
