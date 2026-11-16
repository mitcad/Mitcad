# SPDX-License-Identifier: MIT
# A filament's colour is its appearance (mitcad#1055): Material ▸
# material ▸ appearance, the Materials window's appearances and the
# appearance picker (the driver script shared with the Linux and macOS
# tests).
add_test(NAME app.ui-windows-filament-appearance
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-filament-appearance-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-filament-appearance" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-filament-appearance PROPERTIES TIMEOUT 360)
