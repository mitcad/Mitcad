# SPDX-License-Identifier: MIT
# Press Pull on a face continued (mitcad#1850): a bent tube's end opens
# Revolve1's edit, then goes Along the Walls with its ring dragged past
# the walls' inner radius; a T-slot bar's end Straight with a Scale
# taper. The Linux test's shared input script
# (tools/ui-press-pull-continued-test.sh).
add_test(NAME app.ui-windows-press-pull-continued
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-press-pull-continued-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-press-pull-continued"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-press-pull-continued PROPERTIES TIMEOUT 300)
