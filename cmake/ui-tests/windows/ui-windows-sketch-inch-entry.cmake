# SPDX-License-Identifier: MIT
# Lengths typed in a design in inches are stored as inches (mitcad#1452,
# GitHub mitcad/Mitcad#187): Add User Parameter, a dimension made in mm,
# a rectangle and a circle by typed values. The Linux test's shared
# input script (tools/ui-sketch-inch-entry-test.sh).
add_test(NAME app.ui-windows-sketch-inch-entry
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-inch-entry-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-inch-entry"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-inch-entry PROPERTIES TIMEOUT 180)
