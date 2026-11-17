# SPDX-License-Identifier: MIT
# The selection through the bodies (mitcad#1956): a profile a block hides,
# selected with no command active, and a face of the block turned away
# drawn through it; with Selection Through Bodies off, neither. The Linux
# test's shared input script (tools/ui-selection-through-bodies-test.sh).
add_test(NAME app.ui-windows-selection-through-bodies
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-selection-through-bodies-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-selection-through-bodies"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-selection-through-bodies PROPERTIES TIMEOUT 300)
