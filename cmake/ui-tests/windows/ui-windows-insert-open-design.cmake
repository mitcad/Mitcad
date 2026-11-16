# SPDX-License-Identifier: MIT
# Another open document selected through its real list; save/cancel,
# independent insertion undo and self-insertion exclusion (mitcad#1172).
add_test(NAME app.ui-windows-insert-open-design
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-insert-open-design-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-insert-open-design"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-insert-open-design PROPERTIES TIMEOUT 420)
