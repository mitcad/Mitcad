# SPDX-License-Identifier: MIT
# Preferences › Sketch › Show profiles when a sketch opens (mitcad#1670):
# off, each sketch opens with Show profiles unchecked and no region
# shaded; on (the default), as before. The Linux test's shared input
# script (tools/ui-sketch-profiles-on-open-test.sh).
add_test(NAME app.ui-windows-sketch-profiles-on-open
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-profiles-on-open-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-profiles-on-open"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-profiles-on-open PROPERTIES TIMEOUT 300)
