# SPDX-License-Identifier: MIT
# Trim on an offset's curve (mitcad#1755): the arc copy cut at the line
# across it, the note that the offset is no longer linked to its
# source; the Linux test's shared input script.
add_test(NAME app.ui-windows-sketch-offset-trim
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-offset-trim-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-offset-trim"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-offset-trim PROPERTIES TIMEOUT 300)
