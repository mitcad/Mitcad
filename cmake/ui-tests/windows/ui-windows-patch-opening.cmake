# SPDX-License-Identifier: MIT
# Patch picks a surface body's free edge as its opening and closes it
# (mitcad#1927). The Linux test's shared input script
# (tools/ui-patch-opening-test.sh).
add_test(NAME app.ui-windows-patch-opening
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-patch-opening-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-patch-opening"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-patch-opening PROPERTIES TIMEOUT 240)
