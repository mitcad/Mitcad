# SPDX-License-Identifier: MIT
# A name given to a sketch dimension's parameter taken off again
# (mitcad#1577), and the bare name typed as the value keeps it as a
# user parameter without a refusal (mitcad#1608). The Linux test's
# shared input script (tools/ui-sketch-remove-dimension-name-test.sh).
add_test(NAME app.ui-windows-sketch-remove-dimension-name
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-remove-dimension-name-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-remove-dimension-name"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-remove-dimension-name PROPERTIES TIMEOUT 300)
