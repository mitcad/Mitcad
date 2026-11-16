# SPDX-License-Identifier: MIT
# Move/Copy, Move Object Components, on an imported STEP assembly
# (mitcad#1891): a click on a body of its part takes the import's
# occurrence in the root, which 40 mm along X moves whole. The
# Linux test's shared input script (tools/ui-move-imported-assembly-test.sh).
add_test(NAME app.ui-windows-move-imported-assembly
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-imported-assembly-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-move-imported-assembly"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-imported-assembly PROPERTIES TIMEOUT 240)
