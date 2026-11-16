# SPDX-License-Identifier: MIT
# Repeat runs a value's reaction with the repeated value (mitcad#1844)
# and keeps typed only what was typed (mitcad#1845): a Hole with a
# custom thread repeated, its sizes filled for the repeated Major
# Diameter and its countersink following a new one; a typed countersink
# kept after Repeat. The Linux test's shared input script
# (tools/ui-hole-repeat-test.sh).
add_test(NAME app.ui-windows-hole-repeat
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hole-repeat-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-hole-repeat"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-hole-repeat PROPERTIES TIMEOUT 300)
