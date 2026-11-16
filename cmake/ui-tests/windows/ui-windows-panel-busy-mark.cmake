# SPDX-License-Identifier: MIT
# A preview being computed shown without resizing the panel
# (mitcad#1907, GitHub mitcad/Mitcad#361): a cube's Extrude edited, its
# distance arrow dragged with slowed previews, Docked and Floating; the
# busy mark beside OK and Cancel shows and hides, the panel's size
# stays. The Linux test's shared input script
# (tools/ui-panel-busy-mark-test.sh).
add_test(NAME app.ui-windows-panel-busy-mark
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-panel-busy-mark-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-panel-busy-mark"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-panel-busy-mark PROPERTIES TIMEOUT 400)
