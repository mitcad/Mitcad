# SPDX-License-Identifier: MIT
# The Sketch Palette's Construction (X) box converts the selected curves as
# X does and shows their state (mitcad#2040), shared with Linux and macOS.
add_test(NAME app.ui-windows-sketch-construction-box
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-construction-box-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-construction-box"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-construction-box PROPERTIES TIMEOUT 300)
