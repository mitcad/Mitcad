# SPDX-License-Identifier: MIT
# The sketch of a component whose first occurrence is hidden, edited
# where its shown one places it, its curves drawn (mitcad#1194); the
# Linux test's shared input script.
add_test(NAME app.ui-windows-hidden-occurrence-sketch
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hidden-occurrence-sketch-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-hidden-occurrence-sketch"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-hidden-occurrence-sketch PROPERTIES TIMEOUT 300)
