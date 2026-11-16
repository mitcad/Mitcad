# SPDX-License-Identifier: MIT
# Circular Pattern's Axis 2 (mitcad#1588): a cube about X and Z, its
# rows hidden until Axis 2 is picked, the 12 copies' places, the
# reopened panel with its second axis; every feature of a cubic lattice
# (tools/cli/tests/f4_pattern_lattice.json) opens in its panel.
add_test(NAME app.ui-windows-pattern-two-axes
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-pattern-two-axes-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-pattern-two-axes"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-pattern-two-axes PROPERTIES TIMEOUT 600)
