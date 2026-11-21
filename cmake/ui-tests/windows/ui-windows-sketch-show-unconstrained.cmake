# SPDX-License-Identifier: MIT
# Show Unconstrained (mitcad#2026): the Sketch Palette's button, a click on
# the degrees of freedom badge and the sketch's context menu each select
# the curves and points that are not fully constrained. The Linux test's
# shared input script (tools/ui-sketch-show-unconstrained-test.sh).
add_test(NAME app.ui-windows-sketch-show-unconstrained
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-show-unconstrained-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-show-unconstrained"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-show-unconstrained PROPERTIES TIMEOUT 300)
