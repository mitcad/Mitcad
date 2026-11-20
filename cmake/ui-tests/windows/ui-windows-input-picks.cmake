# SPDX-License-Identifier: MIT
# A selection input's picks listed by name on demand, each highlighted in
# the view on hover (mitcad#2064, GitHub mitcad/Mitcad#473). The Linux
# test's shared input script (tools/ui-input-picks-test.sh).
add_test(NAME app.ui-windows-input-picks
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-input-picks-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-input-picks"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-input-picks PROPERTIES TIMEOUT 300)
