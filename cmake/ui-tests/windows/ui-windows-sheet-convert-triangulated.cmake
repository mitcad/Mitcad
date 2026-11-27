# SPDX-License-Identifier: MIT
# Convert to Sheet Metal by triangulation (mitcad#2442): a shelled
# rectangle-to-round transition's four outer faces picked with Development
# Approximate, OK, the body kept and its Flat Pattern approximate; edited
# with fewer triangles, its stated deviation grows. The Linux test's shared
# input script (tools/ui-sheet-convert-triangulated-test.sh).
add_test(NAME app.ui-windows-sheet-convert-triangulated
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-convert-triangulated-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-convert-triangulated"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-convert-triangulated PROPERTIES TIMEOUT 300)
