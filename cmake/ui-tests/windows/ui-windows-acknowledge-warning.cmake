# SPDX-License-Identifier: MIT
# Acknowledged warnings (mitcad#1935): a warned extrusion's warning
# acknowledged leaves the warnings badge and the yellow cell, and comes
# back with Show Warning Again; the Linux test's shared input script.
add_test(NAME app.ui-windows-acknowledge-warning
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-acknowledge-warning-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-acknowledge-warning"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-acknowledge-warning PROPERTIES TIMEOUT 180)
