# SPDX-License-Identifier: MIT
# Continuity Analysis (mitcad#2521): a tangent rounding's face picked, its
# long edges G1 and its ends G0; the body picked in the browser, a curvature
# continuous rounding's edges G2; a looser curvature tolerance; Escape keeps
# nothing. The Linux test's shared input script
# (tools/ui-continuity-analysis-test.sh).
add_test(NAME app.ui-windows-continuity-analysis
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-continuity-analysis-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-continuity-analysis"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-continuity-analysis PROPERTIES TIMEOUT 360)
