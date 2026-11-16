# SPDX-License-Identifier: MIT
# Inactive sketch constraints (mitcad#721): drawn inactive, Activate
# refused and applied, undo, Delete.
add_test(NAME app.ui-windows-sketch-inactive
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-inactive-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-inactive"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-inactive PROPERTIES TIMEOUT 300)
