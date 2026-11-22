# SPDX-License-Identifier: MIT
# A panel's preview held while the model finishes another job shows the
# busy mark (mitcad#2118): a cube's Extrude edited, a new Distance typed
# while its preview's refinement is held, Docked and Floating. The Linux
# test's shared input script (tools/ui-panel-busy-waiting-test.sh).
add_test(NAME app.ui-windows-panel-busy-waiting
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-panel-busy-waiting-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-panel-busy-waiting"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-panel-busy-waiting PROPERTIES TIMEOUT 300)
