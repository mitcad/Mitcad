# SPDX-License-Identifier: MIT
# The Parameters window's empty row, its rows' delete buttons and its
# bottom row (mitcad#2202, GitHub mitcad/Mitcad#512): parameters typed one
# after the other in the empty row, a taken name refused, an unused
# parameter deleted by its row's button and a used one's button disabled,
# the empty row last under a sort and hidden under the search, a slider's
# value shown during the drag, the bottom row's pictures on both tabs. The
# Linux test's shared input script (tools/ui-parameters-new-row-test.sh).
add_test(NAME app.ui-windows-parameters-new-row
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-parameters-new-row-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-parameters-new-row"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-parameters-new-row PROPERTIES TIMEOUT 300)
