# SPDX-License-Identifier: MIT
# The Loft panel's Tangent Edges (mitcad#2162): Merge by default, a circle
# to a slot gives one side face. The Linux test's shared input script
# (tools/ui-loft-tangent-edges-test.sh).
add_test(NAME app.ui-windows-loft-tangent-edges
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-loft-tangent-edges-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-loft-tangent-edges"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-loft-tangent-edges PROPERTIES TIMEOUT 300)
