# SPDX-License-Identifier: MIT
# Tables in Change Parameters (mitcad#667): a table made and typed in
# (a wrong cell refused), a dimension through XLOOKUP with the Formula
# Editor's completion of Screws., a cell edit the block follows, a CSV
# imported through its preview.
add_test(NAME app.ui-windows-tables
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-tables-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-tables"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-tables PROPERTIES TIMEOUT 300)
