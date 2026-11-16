# SPDX-License-Identifier: MIT
# The BUILD lists' Copy as Text and Save as Text..., CSV with CRLF rows
# (mitcad#1641). The Linux test's shared input script
# (tools/ui-build-lists-text-test.sh).
add_test(NAME app.ui-windows-build-lists-text
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-lists-text-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-lists-text"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-lists-text PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)
