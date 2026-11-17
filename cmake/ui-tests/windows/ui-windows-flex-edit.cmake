# SPDX-License-Identifier: MIT
# A Flex item edited directly in the view (mitcad#1713): its handles
# shown for the selected item, a node dragged with a preview as one
# command and one undo step, a node added on the route and deleted, a
# new end node from the "+", Esc. The Linux test is
# tools/ui-flex-edit-test.sh.
add_test(NAME app.ui-windows-flex-edit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-flex-edit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-flex-edit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-flex-edit PROPERTIES TIMEOUT 300)
