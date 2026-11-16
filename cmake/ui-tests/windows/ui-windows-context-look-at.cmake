# SPDX-License-Identifier: MIT
# Look At in the view's context menu for a planar face of a component,
# not for a cylindrical face (mitcad#1212); the Linux test's shared
# input script.
add_test(NAME app.ui-windows-context-look-at
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-context-look-at-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-context-look-at"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-context-look-at PROPERTIES TIMEOUT 180)
