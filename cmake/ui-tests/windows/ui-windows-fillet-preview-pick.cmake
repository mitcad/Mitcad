# SPDX-License-Identifier: MIT
# A pick on Fillet's preview names the edge before the fillet it comes
# from (mitcad#1458, GitHub mitcad/Mitcad#177): Fillet1, rounding a
# block's end foot edges on a plate, edited; the front's foot edge
# between the roundings, clicked in the preview without Ctrl, joins
# Edges and the three roundings warn of nothing hanging out. The Linux
# test's shared input script (tools/ui-fillet-preview-pick-test.sh).
add_test(NAME app.ui-windows-fillet-preview-pick
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-preview-pick-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-preview-pick"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fillet-preview-pick PROPERTIES TIMEOUT 300)
