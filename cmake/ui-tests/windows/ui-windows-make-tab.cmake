# SPDX-License-Identifier: MIT
# The Make workspace's tabs (mitcad#1049; 3D Print, Laser and CNC): their
# groups and menus, the moved commands from them, the Annotate group's
# kinds, the body card and the Browser's markers; Floating too.
add_test(NAME app.ui-windows-make-tab
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-make-tab-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-make-tab"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-make-tab PROPERTIES TIMEOUT 300)
