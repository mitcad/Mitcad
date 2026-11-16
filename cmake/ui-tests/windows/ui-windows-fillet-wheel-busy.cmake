# SPDX-License-Identifier: MIT
# The mouse wheel over Fillet's Radius while its preview computes
# (mitcad#1407, GitHub mitcad/Mitcad#162): each notch steps the field
# without reading the document, the last value is previewed once the
# job ends and OK makes that fillet. The Linux test's shared input
# script (tools/ui-fillet-wheel-busy-test.sh).
add_test(NAME app.ui-windows-fillet-wheel-busy
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-wheel-busy-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-wheel-busy"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fillet-wheel-busy PROPERTIES TIMEOUT 240)
