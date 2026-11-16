# SPDX-License-Identifier: MIT
# The rolled-back features evaluated in the background after an edit
# before the marker, so that the marker dragged to the end evaluates
# nothing; an edit that opens cancels a run at once (mitcad#1267); the
# Linux test's shared input script.
add_test(NAME app.ui-windows-background-evaluation
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-background-evaluation-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-background-evaluation"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-background-evaluation PROPERTIES TIMEOUT 300)
