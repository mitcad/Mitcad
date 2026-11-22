# SPDX-License-Identifier: MIT
# Selection sets (mitcad#2192): a body, a used sketch and a feature into a
# new set from the Browser's row menu, renamed in place; the set's row
# selects them; Find in Timeline and Find in Browser from member rows;
# Remove from Set, Delete and Undo; kept with the design. The Linux test's
# shared input script (tools/ui-selection-sets-test.sh).
add_test(NAME app.ui-windows-selection-sets
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-selection-sets-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-selection-sets"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-selection-sets PROPERTIES TIMEOUT 300)
