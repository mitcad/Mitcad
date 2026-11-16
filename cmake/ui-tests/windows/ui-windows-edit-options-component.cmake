# SPDX-License-Identifier: MIT
# Edit Options of an inserted library part (mitcad#1929): Width changed
# in the dialog is previewed without a refusal and kept by OK in one
# undo step, on a synthetic library (git). The Linux test's shared
# input script (tools/ui-edit-options-component-test.sh).
add_test(NAME app.ui-windows-edit-options-component
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-edit-options-component-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-edit-options-component"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-edit-options-component PROPERTIES TIMEOUT 300)
