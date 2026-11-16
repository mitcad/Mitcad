# SPDX-License-Identifier: MIT
# 3D Print's Import from Slicer... (mitcad#654, mitcad#655): a Bambu Studio
# project whose filaments are slowed for cooling on overhangs from 95%
# imported; the line under the button counts by kind, Details... shows
# the whole report.
add_test(NAME app.ui-windows-print-settings
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-settings-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-print-settings" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-print-settings PROPERTIES TIMEOUT 300)
