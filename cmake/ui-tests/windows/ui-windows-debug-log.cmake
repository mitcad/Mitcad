# SPDX-License-Identifier: MIT
# The debug log (mitcad#269): --debug-log, its header and lines,
# Preferences' switch on and off without a restart, Open Log Folder;
# Create Sketch by its default Alt+S (mitcad#267).
add_test(NAME app.ui-windows-debug-log
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-debug-log-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-debug-log"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-debug-log PROPERTIES TIMEOUT 300)
