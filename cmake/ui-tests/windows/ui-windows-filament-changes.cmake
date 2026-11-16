# SPDX-License-Identifier: MIT
# Send to Slicer's Filament changes (mitcad#1643): a change at a typed
# height, the view's print colours, Undo, a change at a picked plane,
# Filament Change Here…. The Linux test's shared input script
# (tools/ui-filament-changes-test.sh).
add_test(NAME app.ui-windows-filament-changes
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-filament-changes-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-filament-changes"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-filament-changes PROPERTIES TIMEOUT 300)
