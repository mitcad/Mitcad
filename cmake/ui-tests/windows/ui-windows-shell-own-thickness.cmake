# SPDX-License-Identifier: MIT
# Shell with faces of their own thickness (mitcad#2443): the Linux test's
# shared input script (tools/ui-shell-own-thickness-test.sh,
# tools/ui-scripts/shell-own-thickness.mitcad-ui).
add_test(NAME app.ui-windows-shell-own-thickness
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-shell-own-thickness-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-shell-own-thickness"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-shell-own-thickness PROPERTIES TIMEOUT 240)
