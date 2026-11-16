# SPDX-License-Identifier: MIT
# The Sketch Palette's Create section (mitcad#1027): the defaults, the
# constraints on two rows, nothing clipped, a tool started three times
# there when the sketch opens again, its button starting it checked.
add_test(NAME app.ui-windows-sketch-palette-create
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-palette-create-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-palette-create" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-palette-create PROPERTIES TIMEOUT 300)
