# SPDX-License-Identifier: MIT
# The Properties window (mitcad#1816): several bodies' summary and
# merged rows, a row's bodies painted under the pointer, a row's
# details, Copy's tab-separated text and one body's groups with More
# collapsed. The Linux test's shared input script
# (tools/ui-properties-window-test.sh).
add_test(NAME app.ui-windows-properties-window
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-properties-window-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-properties-window"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-properties-window PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)
