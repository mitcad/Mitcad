# SPDX-License-Identifier: MIT
# A feature that changes the picked body activates its component
# (mitcad#1634): Fillet on another component's edge activates it (Undo:
# the fillet, then the activation; Cancel keeps it), and an edge of each
# component offers Activate in the panel. The Linux test's shared input
# script (tools/ui-activate-on-pick-test.sh).
add_test(NAME app.ui-windows-activate-on-pick
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-activate-on-pick-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-activate-on-pick"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-activate-on-pick PROPERTIES TIMEOUT 300)
