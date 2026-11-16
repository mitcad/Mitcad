# SPDX-License-Identifier: MIT
# The joint highlight (mitcad#523): a face-to-face joint and a circular
# edge's joint selected in the browser and the timeline, each side's
# geometry, frames and occurrences logged, another joint replacing it,
# a click on nothing clearing it, a row's hover previewing it; a BUILD
# joint's mount frames with their parts.
add_test(NAME app.ui-windows-joint-highlight
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-joint-highlight-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-joint-highlight"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-joint-highlight PROPERTIES TIMEOUT 300)
