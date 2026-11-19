# SPDX-License-Identifier: MIT
# Panelize (mitcad#2005): the shared steps of tools/ui-scripts/panelize.mitcad-ui,
# a box made a cabinet of 18 mm panels in three steps, the last with Remove
# Remainder (tools/ui-panelize-test.sh).
add_test(NAME app.ui-windows-panelize
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-panelize-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-panelize"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-panelize PROPERTIES TIMEOUT 600)
