# SPDX-License-Identifier: MIT
# Isoparametric Curve (mitcad#2522): the Linux test's shared input script
# (tools/ui-isoparametric-curve-test.sh,
# tools/ui-scripts/isoparametric-curve.mitcad-ui).
add_test(NAME app.ui-windows-isoparametric-curve
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-isoparametric-curve-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-isoparametric-curve"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-isoparametric-curve PROPERTIES TIMEOUT 240)
