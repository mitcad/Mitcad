# SPDX-License-Identifier: MIT
# Annotations (mitcad#702): a tolerance from the editor opened in the
# view, the text dragged, an H7 fit, several changed together in the
# Annotations window, a group hidden, a snapshot of one group.
add_test(NAME app.ui-windows-annotations
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-annotations-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-annotations"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-annotations PROPERTIES TIMEOUT 300)
