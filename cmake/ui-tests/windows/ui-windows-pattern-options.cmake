# SPDX-License-Identifier: MIT
# The patterns' Direction Type and Distance Type (mitcad#545): a
# rectangular pattern symmetric along X and by extent along Y, a
# circular one symmetric, the stored definitions and the copies' places,
# the panels of the reopened patterns showing their options.
add_test(NAME app.ui-windows-pattern-options
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-pattern-options-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-pattern-options"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-pattern-options PROPERTIES TIMEOUT 300)
