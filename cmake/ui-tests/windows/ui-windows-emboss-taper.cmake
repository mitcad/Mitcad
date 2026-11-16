# SPDX-License-Identifier: MIT
# Emboss's Taper Angle (mitcad#1547): a square wrapped onto a rod with
# Taper Angle 10 typed in the panel, previewed and kept by OK. The Linux
# test's shared input script (tools/ui-emboss-taper-test.sh).
add_test(NAME app.ui-windows-emboss-taper
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-emboss-taper-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-emboss-taper"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-emboss-taper PROPERTIES TIMEOUT 240)
