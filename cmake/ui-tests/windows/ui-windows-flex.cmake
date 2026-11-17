# SPDX-License-Identifier: MIT
# Flex laid by clicks (mitcad#1713): a hose from the end port of a pipe
# over two faces of a block, free nodes, Backspace, a double click as
# the end and the twist of a flat section, in the Docked and the
# Floating layout; the Linux test's shared input script
# (tools/ui-flex-test.sh).
add_test(NAME app.ui-windows-flex
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-flex-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-flex"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-flex PROPERTIES TIMEOUT 900)
