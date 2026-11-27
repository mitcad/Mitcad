# SPDX-License-Identifier: MIT
# A base view with many warnings (mitcad#2507, GitHub mitcad/Mitcad#694):
# selected, the drawing closed and opened again without a hang watchdog
# line, the warnings said once with their count. The Linux test's shared
# input script (tools/ui-drawing-many-warnings-test.sh,
# tools/ui-scripts/drawing-many-warnings.mitcad-ui).
add_test(NAME app.ui-windows-drawing-many-warnings
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-many-warnings-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-many-warnings"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-many-warnings PROPERTIES TIMEOUT 300)
