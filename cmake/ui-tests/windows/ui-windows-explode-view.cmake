# SPDX-License-Identifier: MIT
# Exploded views (mitcad#2113): Auto Explode in the Explode panel, a step
# by Explode Step's Distance arrow, the steps played back and forth, the
# Browser's row showing the design assembled and exploded, a drawing's base
# view with the explode view; Measure on the exploded bodies and a step
# along a picked edge (mitcad#2328); kept with the design. The Linux test's
# shared input script (tools/ui-explode-view-test.sh).
add_test(NAME app.ui-windows-explode-view
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-explode-view-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-explode-view"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-explode-view PROPERTIES TIMEOUT 480)
