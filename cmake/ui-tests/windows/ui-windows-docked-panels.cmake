# SPDX-License-Identifier: MIT
# The Docked layout's Command and Projects docks hidden from the
# window's menu of its panels and with their close buttons, shown again
# from the View menu; no Configuration in that menu (mitcad#1455); the
# Linux test's shared input script.
add_test(NAME app.ui-windows-docked-panels
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-panels-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-panels"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-docked-panels PROPERTIES TIMEOUT 180)
