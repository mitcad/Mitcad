# SPDX-License-Identifier: MIT
# A design as it opens (mitcad#426): the root active whatever the file
# had active, unmodified; a tree longer than the browser folded below
# the root, a small one expanded; Collapse All keeps the root open.
add_test(NAME app.ui-windows-browser-open
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-open-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-open"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-open PROPERTIES TIMEOUT 300)
