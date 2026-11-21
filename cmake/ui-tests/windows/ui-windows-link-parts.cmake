# SPDX-License-Identifier: MIT
# Linked parts of a design (mitcad#2130): Insert Component > Linked >
# Choose parts... with one component ticked links it alone; Choose Linked
# Parts... adds a second one, and Undo takes it back. The Linux test's
# shared input script (tools/ui-link-parts-test.sh).
add_test(NAME app.ui-windows-link-parts
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-link-parts-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-link-parts"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-link-parts PROPERTIES TIMEOUT 240)
