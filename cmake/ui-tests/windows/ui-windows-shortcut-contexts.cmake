# SPDX-License-Identifier: MIT
# Keys per context (mitcad#720): Keyboard Shortcuts driven by its keys
# (Command Search F3 in Sketch, Fit Point Spline S, OK; then a conflict
# OK refuses); S draws a spline in a sketch and opens the command search
# in the model, F3 searches in the sketch.
add_test(NAME app.ui-windows-shortcut-contexts
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-shortcut-contexts-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-shortcut-contexts"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-shortcut-contexts PROPERTIES TIMEOUT 300)
