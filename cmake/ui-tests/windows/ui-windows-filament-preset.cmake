# SPDX-License-Identifier: MIT
# A material's filament preset per slicer (mitcad#1062): 3D Print warns
# under the Material row of a material that names no preset of the
# slicer, Bambu Studio's typed on Materials' Filament tab (Edit
# materials… of the row's picker, mitcad#1141), the 3MF sent names
# the filament by it and inherits from it.
add_test(NAME app.ui-windows-filament-preset
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-filament-preset-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-filament-preset" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-filament-preset PROPERTIES TIMEOUT 300)
