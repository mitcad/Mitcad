# SPDX-License-Identifier: MIT
# One filament, its variants per printer and nozzle (mitcad#1054): 3D
# Print's Materials row, Import Presets grouped by filament, the
# Materials window's Variants tab (the driver script shared with the
# Linux and macOS tests).
add_test(NAME app.ui-windows-filament-variants
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-filament-variants-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-filament-variants" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-filament-variants PROPERTIES TIMEOUT 360)
