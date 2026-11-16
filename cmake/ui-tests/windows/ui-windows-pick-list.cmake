# SPDX-License-Identifier: MIT
# The pick list (mitcad#1703): a press held still lists the profile and
# the face behind it, nearest first; a row hovered, chosen, added with
# Ctrl, Esc and a click elsewhere, a row into Extrude's input; the Linux
# test's shared input script (tools/ui-pick-list-test.sh).
add_test(NAME app.ui-windows-pick-list
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-pick-list-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-pick-list"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-pick-list PROPERTIES TIMEOUT 300)
