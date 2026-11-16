# SPDX-License-Identifier: MIT
# A sketch value editor's error goes when the editor closes, from the
# status line and the message band (mitcad#1621). The Linux test's
# shared input script (tools/ui-sketch-value-error-clears-test.sh).
add_test(NAME app.ui-windows-sketch-value-error-clears
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-value-error-clears-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-value-error-clears"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-value-error-clears PROPERTIES TIMEOUT 300)
