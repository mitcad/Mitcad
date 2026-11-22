# SPDX-License-Identifier: MIT
# Draft Analysis (mitcad#2166): the panel's three classes pulled along +Z,
# at 3 degrees, flipped and along a construction plane picked in the
# browser; kept as Draft1, hidden, shown, edited, saved, deleted and undone.
# The Linux test's shared input script (tools/ui-draft-analysis-test.sh).
add_test(NAME app.ui-windows-draft-analysis
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-draft-analysis-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-draft-analysis"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-draft-analysis PROPERTIES TIMEOUT 360)
