# SPDX-License-Identifier: MIT
# A parameter error put right leaves the message band, and expression
# errors in words, checked while the Parameters window's empty row is
# typed in (mitcad#2181, mitcad#2202). The Linux test's shared input script
# (tools/ui-parameter-error-put-right-test.sh).
add_test(NAME app.ui-windows-parameter-error-put-right
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-parameter-error-put-right-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-parameter-error-put-right"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-parameter-error-put-right PROPERTIES TIMEOUT 300)
