# SPDX-License-Identifier: MIT
# A constraint symbol keeps clear of the curves (mitcad#1594): with two
# close parallel lines, a click on the upper line's middle selects the
# line, not the Parallel whose second symbol covered it, and both of its
# symbols are in the glyph log. The Linux test's shared input script
# (tools/ui-sketch-glyph-clear-test.sh).
add_test(NAME app.ui-windows-sketch-glyph-clear
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-glyph-clear-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-glyph-clear"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-glyph-clear PROPERTIES TIMEOUT 300)
