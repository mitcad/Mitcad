# SPDX-License-Identifier: MIT
# Dimension to a pipe bend's curved outline (its top, an extreme point
# of a silhouette) and a section's label dragged and put back
# (mitcad#1909). The Linux test's shared input script
# (tools/ui-drawing-curved-outline-test.sh).
add_test(NAME app.ui-windows-drawing-curved-outline
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-curved-outline-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-curved-outline"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-curved-outline PROPERTIES TIMEOUT 240)
