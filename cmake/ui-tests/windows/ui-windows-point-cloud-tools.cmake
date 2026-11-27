# SPDX-License-Identifier: MIT
# Point cloud tools (mitcad#2516): the shared steps of
# tools/ui-scripts/point-cloud-tools.mitcad-ui, a scan filtered, aligned
# onto a block with its remaining deviation stated and rebuilt as a mesh
# body, saved (tools/ui-point-cloud-tools-test.sh).
add_test(NAME app.ui-windows-point-cloud-tools
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-point-cloud-tools-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-point-cloud-tools"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-point-cloud-tools PROPERTIES TIMEOUT 900)
