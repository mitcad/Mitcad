# SPDX-License-Identifier: MIT
# The BOM Structure and the Bill of Materials' structured and parts-only
# views (mitcad#1985). The Linux test's shared input script
# (tools/ui-bom-structure-test.sh).
add_test(NAME app.ui-windows-bom-structure
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-bom-structure-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-bom-structure"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-bom-structure PROPERTIES TIMEOUT 300)
