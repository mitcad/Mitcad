# SPDX-License-Identifier: MIT
# The Bill of Materials' Kind and BOM Structure, its filters, the Sheet
# Metal tab and its Flat Patterns on a cut sheet (mitcad#1980). The Linux
# test's shared input script (tools/ui-bom-kinds-test.sh).
add_test(NAME app.ui-windows-bom-kinds
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-bom-kinds-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-bom-kinds"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-bom-kinds PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)
