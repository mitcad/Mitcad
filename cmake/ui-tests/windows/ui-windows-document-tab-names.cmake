# SPDX-License-Identifier: MIT
# The document tabs' names in the Floating top bar (mitcad#2060): Bracket
# and Untitled shown whole, a 60-character name elided. The Linux test's
# shared input script (tools/ui-document-tab-names-test.sh).
add_test(NAME app.ui-windows-document-tab-names
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-document-tab-names-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-document-tab-names"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-document-tab-names PROPERTIES TIMEOUT 300)
