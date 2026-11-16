# SPDX-License-Identifier: MIT
# Edit Parameters one click away in every workspace (mitcad#1582,
# GitHub mitcad/Mitcad#222): the navigation bar's button in SOLID,
# BUILD, MAKE and a sketch, the drawing bar's in a drawing, Floating and
# Docked. The Linux test's shared input script
# (tools/ui-parameters-everywhere-test.sh).
add_test(NAME app.ui-windows-parameters-everywhere
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-parameters-everywhere-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-parameters-everywhere"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-parameters-everywhere PROPERTIES TIMEOUT 420)
