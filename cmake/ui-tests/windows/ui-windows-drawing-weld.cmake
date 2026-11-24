# SPDX-License-Identifier: MIT
# Weld symbols on a drawing (mitcad#2350): Weld Symbol on an edge, Propose
# Welds on a two-member BUILD frame's T joint. The Linux test's shared input
# script (tools/ui-drawing-weld-test.sh), with a synthetic BUILD library
# fetched from a local git repository.
add_test(NAME app.ui-windows-drawing-weld
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-weld-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-weld"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-weld PROPERTIES TIMEOUT 300)
