# SPDX-License-Identifier: MIT
# Auto Fix in the timeline's context menu (mitcad#1223): an extrusion
# whose region a sketch change split is fixed as one undo step and its
# warning goes; the Linux test's shared input script.
add_test(NAME app.ui-windows-timeline-auto-fix
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-timeline-auto-fix-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-timeline-auto-fix"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-timeline-auto-fix PROPERTIES TIMEOUT 180)
