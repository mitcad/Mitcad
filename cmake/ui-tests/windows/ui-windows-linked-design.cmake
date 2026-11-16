# SPDX-License-Identifier: MIT
# Linked designs while the holder is open (mitcad#1416): the notice with
# Open Linked Design on an edit of a linked component, the linked design
# opened in a tab of its own, "<file> changed" with Update when it is
# saved in its tab, Update Link of the context menu. The Linux test is
# tools/ui-linked-design-test.sh.
add_test(NAME app.ui-windows-linked-design
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-linked-design-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-linked-design" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-linked-design PROPERTIES TIMEOUT 420)
