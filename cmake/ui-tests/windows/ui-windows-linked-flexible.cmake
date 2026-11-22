# SPDX-License-Identifier: MIT
# A design linked with its components made Flexible (mitcad#2209): Flexible
# offered on the linked sub-assembly's Browser row and its slider part
# dragged along the design's own joint, for that occurrence alone. The
# Linux test's shared input script (tools/ui-linked-flexible-test.sh).
add_test(NAME app.ui-windows-linked-flexible
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-linked-flexible-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-linked-flexible"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-linked-flexible PROPERTIES TIMEOUT 300)
