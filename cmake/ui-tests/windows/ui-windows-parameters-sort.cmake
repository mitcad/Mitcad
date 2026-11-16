# SPDX-License-Identifier: MIT
# Edit Parameters sorted by a column (mitcad#1866, GitHub
# mitcad/Mitcad#338): Name's and Value's headers sort the rows within
# their groups, both ways and back to the creation order; the choice is
# kept; an edited and an added row stay selected in their place. The
# Linux test's shared input script (tools/ui-parameters-sort-test.sh).
add_test(NAME app.ui-windows-parameters-sort
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-parameters-sort-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-parameters-sort"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-parameters-sort PROPERTIES TIMEOUT 300)
