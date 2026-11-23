# SPDX-License-Identifier: MIT
# Cancel Sketch takes back everything done in a sketch since it was
# opened (an edited sketch, a new one, a 3D sketch), Redo brings it back
# in one step, and a sketch edit is one undo step at Finish Sketch
# (mitcad#2225). The Linux test's shared input script
# (tools/ui-sketch-cancel-test.sh).
add_test(NAME app.ui-windows-sketch-cancel
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-cancel-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-cancel"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-cancel PROPERTIES TIMEOUT 300)
