# SPDX-License-Identifier: MIT
# Print bed face (mitcad#731): set from a face's context menu, shown
# from another face's, cleared and undone; 3D Print turns the box onto
# it; Print Analysis starts from it; a face that is gone is reported.
add_test(NAME app.ui-windows-print-bed
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-bed-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-print-bed" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-print-bed PROPERTIES TIMEOUT 300)
