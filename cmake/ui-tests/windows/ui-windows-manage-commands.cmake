# SPDX-License-Identifier: MIT
# The Manage tab's Recompute All, Units and Plastic Rules (mitcad#1982),
# each run from its group's menu on the MANAGE tab; the Linux test's shared
# input script (tools/ui-manage-commands-test.sh).
add_test(NAME app.ui-windows-manage-commands
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-manage-commands-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-manage-commands"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-manage-commands PROPERTIES TIMEOUT 180)
