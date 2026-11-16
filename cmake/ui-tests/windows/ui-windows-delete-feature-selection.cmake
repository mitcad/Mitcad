# SPDX-License-Identifier: MIT
# Selection Delete, dependency union, Cancel and one-step Undo (mitcad#1139).
add_test(NAME app.ui-windows-delete-feature-selection
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-delete-feature-selection-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-delete-feature-selection"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-delete-feature-selection PROPERTIES TIMEOUT 300)
