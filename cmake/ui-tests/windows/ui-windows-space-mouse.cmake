# SPDX-License-Identifier: MIT
# A 3D mouse (mitcad#951): no device in the VM logs "3D mouse: none";
# the test driver's 6-DOF input turns, pans and zooms the view, and the
# first button runs Fit, and Lock Rotation on the second locks and frees
# the rotation, also in a sketch (mitcad#1910; the script shared with the
# Linux and macOS tests).
add_test(NAME app.ui-windows-space-mouse
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-space-mouse-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-space-mouse"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-space-mouse PROPERTIES TIMEOUT 300)
