# SPDX-License-Identifier: MIT
# Typed polygon radii/first-edge lengths, sides-only input and atomic
# Undo/Redo (mitcad#1136), shared with the Linux and macOS driver.
add_test(NAME app.ui-windows-sketch-polygon-dimensions
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-polygon-dimensions-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-polygon-dimensions"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-polygon-dimensions PROPERTIES TIMEOUT 300)
