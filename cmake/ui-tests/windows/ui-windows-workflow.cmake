# SPDX-License-Identifier: MIT
# Input-driven: sketch, extrude, fillet, save, open, STEP export and
# import with mouse and keyboard messages posted to the window.
add_test(NAME app.ui-windows-workflow
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-workflow-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-workflow"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-workflow PROPERTIES TIMEOUT 300)
