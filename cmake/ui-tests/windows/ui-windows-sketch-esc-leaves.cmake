# SPDX-License-Identifier: MIT
# Esc leaves a sketch nothing was changed in, a new empty one taken
# back, and keeps a changed one open with a hint once (mitcad#1623).
# The Linux test's shared input script
# (tools/ui-sketch-esc-leaves-test.sh).
add_test(NAME app.ui-windows-sketch-esc-leaves
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-esc-leaves-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-esc-leaves"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-esc-leaves PROPERTIES TIMEOUT 300)
