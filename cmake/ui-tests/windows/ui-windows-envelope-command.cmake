# SPDX-License-Identifier: MIT
# The Envelope command (mitcad#1276): Envelope of a component at 20 mm
# in the Docked layout, edited and undone, and in the Floating layout;
# Show as Envelope... on one of two occurrences of a linked machine
# leaves the other in full.
add_test(NAME app.ui-windows-envelope-command
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-envelope-command-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-envelope-command"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-envelope-command PROPERTIES TIMEOUT 420)
