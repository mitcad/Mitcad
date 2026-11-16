# SPDX-License-Identifier: MIT
# The browser's scrolling with a selection (mitcad#318): a pick in the
# view scrolls to its row out of sight; a Ctrl+click on a row below the
# first selected one, scrolled away, keeps the scroll position.
add_test(NAME app.ui-windows-browser
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browser"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser PROPERTIES TIMEOUT 300)
