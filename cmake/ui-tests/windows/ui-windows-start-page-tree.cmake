# SPDX-License-Identifier: MIT
# The Start page's Tree / Cards toggle (mitcad#2093): a project's folders
# as a tree, the chosen folder's cards, a card that opens its design, Cards
# at the same folder, the keyboard, and the choice after a restart; the
# Linux test's shared input scripts (tools/ui-start-page-tree-test.sh).
add_test(NAME app.ui-windows-start-page-tree
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-start-page-tree-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-start-page-tree"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-start-page-tree PROPERTIES TIMEOUT 300)
