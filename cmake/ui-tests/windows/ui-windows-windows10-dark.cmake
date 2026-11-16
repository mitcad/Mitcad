# SPDX-License-Identifier: MIT
# Windows 10's dark app mode (mitcad#1282, GitHub mitcad/Mitcad#138):
# with the Windows 10 path forced over its default style, the Start
# page and the Browser dark in the dark app mode and the Start page
# light in the light one, Docked and Floating; without it, Windows 11's
# own style as before.
add_test(NAME app.ui-windows-windows10-dark
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-windows10-dark-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-windows10-dark"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-windows10-dark PROPERTIES TIMEOUT 300)
