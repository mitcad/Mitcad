# SPDX-License-Identifier: MIT
# An Unfold refused by a fillet on a surface edge: the edited Unfold's
# panel names the fillet in words, without ids (mitcad#1942). The Linux
# test's shared input script (tools/ui-sheet-unfold-fillet-test.sh).
add_test(NAME app.ui-windows-sheet-unfold-fillet
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-unfold-fillet-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-unfold-fillet"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-unfold-fillet PROPERTIES TIMEOUT 300)
