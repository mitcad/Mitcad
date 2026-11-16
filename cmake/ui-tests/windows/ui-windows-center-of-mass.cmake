# SPDX-License-Identifier: MIT
# The Center of Mass marker (mitcad#1094): drawn at the properties
# query's total, moved with a component, hidden by its row's light bulb,
# kept hidden in the saved design, deleted from the row's menu.
add_test(NAME app.ui-windows-center-of-mass
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-center-of-mass-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-center-of-mass" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-center-of-mass PROPERTIES TIMEOUT 300)
