# SPDX-License-Identifier: MIT
# A pattern of bodies drawn from its body's meshes, and window
# selections over many bodies decided by their boxes first
# (mitcad#1275).
add_test(NAME app.ui-windows-window-bodies
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-window-bodies-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-window-bodies"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-window-bodies PROPERTIES TIMEOUT 300)
