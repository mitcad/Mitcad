# SPDX-License-Identifier: MIT
# Conic Curve and Edit Rho (mitcad#2236): a conic drawn by three clicks
# with Rho 0.7 typed, then its Rho changed from its right-click menu. The
# Linux test's shared input script (tools/ui-sketch-conic-test.sh).
add_test(NAME app.ui-windows-sketch-conic
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-conic-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-conic"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-conic PROPERTIES TIMEOUT 300)
