# SPDX-License-Identifier: MIT
# Move/Copy's Point to Point along one axis (mitcad#2339): an edit shows
# Along's icon buttons, Along Z moves the cylinder up onto the block's top
# level only; the Linux test's shared input script.
add_test(NAME app.ui-windows-move-along
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-along-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-move-along"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-along PROPERTIES TIMEOUT 300)
