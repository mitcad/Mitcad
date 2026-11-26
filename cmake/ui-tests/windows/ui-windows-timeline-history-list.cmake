# SPDX-License-Identifier: MIT
# The timeline's History list (mitcad#2427): Preferences › Display ›
# Timeline: History list puts the timeline under the Browser in the
# Floating and the Docked layout; its rows in timeline order, the marker
# dragged between them, a group folded, a sketch's eye, Rename in a row, and
# the strip back. The Linux test's shared input scripts
# (tools/ui-timeline-history-list-test.sh).
add_test(NAME app.ui-windows-timeline-history-list
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-timeline-history-list-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-timeline-history-list"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-timeline-history-list PROPERTIES TIMEOUT 420)
