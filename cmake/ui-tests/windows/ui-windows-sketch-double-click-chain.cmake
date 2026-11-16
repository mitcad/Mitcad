# SPDX-License-Identifier: MIT
# A double-click on a sketch curve selects its closed loop past a
# corner a line leaves, else its open chain; Ctrl adds (mitcad#1541,
# GitHub mitcad/Mitcad#204). The Linux test's shared input script
# (tools/ui-sketch-double-click-chain-test.sh).
add_test(NAME app.ui-windows-sketch-double-click-chain
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-double-click-chain-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-double-click-chain"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-double-click-chain PROPERTIES TIMEOUT 180)
