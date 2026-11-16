# SPDX-License-Identifier: MIT
# Assembly Constraint's preview with more than one row (mitcad#1813):
# an incomplete row left out of the preview, which still places the
# part by the complete rows; the part drawn once where the preview puts
# it. The Linux test's shared input script
# (tools/ui-constraint-rows-preview-test.sh).
add_test(NAME app.ui-windows-constraint-rows-preview
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-constraint-rows-preview-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-constraint-rows-preview"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-constraint-rows-preview PROPERTIES TIMEOUT 300)
