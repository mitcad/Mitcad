# SPDX-License-Identifier: MIT
# Surface quality analyses (mitcad#2517): the shared steps of
# tools/ui-scripts/surface-analysis.mitcad-ui, Zebra, Curvature Map,
# Isocurve Analysis and Environment Map on a cylinder, kept and saved
# (tools/ui-surface-analysis-test.sh).
add_test(NAME app.ui-windows-surface-analysis
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-surface-analysis-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-surface-analysis"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-surface-analysis PROPERTIES TIMEOUT 600)
