# SPDX-License-Identifier: MIT
# Resting the pointer on a sketch line acquires it for its extension
# (mitcad#2035): a line ended on a line's extension, a rectangle's corner
# where two acquired lines' extensions meet, each held by Coincident, and
# nothing acquired with Snap to line extensions off. The Linux test's
# shared input script (tools/ui-sketch-line-extensions-test.sh).
add_test(NAME app.ui-windows-sketch-line-extensions
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-line-extensions-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-line-extensions"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-line-extensions PROPERTIES TIMEOUT 180)
