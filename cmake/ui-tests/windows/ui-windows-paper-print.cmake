# SPDX-License-Identifier: MIT
# File > Print... (mitcad#1089): disabled with nothing to print, enabled
# in a sketch, its panel opened and cancelled; a 300 x 200 mm sketch at
# 1:1 printed to a PDF file in place of the system's dialog (two A4
# pages), fit to page (one).
add_test(NAME app.ui-windows-paper-print
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-paper-print-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-paper-print"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-paper-print PROPERTIES TIMEOUT 300)
