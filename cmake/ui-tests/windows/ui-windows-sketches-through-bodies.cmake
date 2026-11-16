# SPDX-License-Identifier: MIT
# Sketches through the bodies outside sketch mode (mitcad#1931): the
# profile a block hides drawn through it, picked in Extrude and joined;
# Show All Sketches, Sketches Through Bodies off and on, Hide All
# Sketches. The Linux test's shared input script
# (tools/ui-sketches-through-bodies-test.sh).
add_test(NAME app.ui-windows-sketches-through-bodies
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketches-through-bodies-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketches-through-bodies"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketches-through-bodies PROPERTIES TIMEOUT 300)
