# SPDX-License-Identifier: MIT
# File > Open from Project (mitcad#530): in the File menu after Open;
# two projects listed, a folder and its sub-folder gone into and back,
# the search finding the other project's design, Open and Open
# Read-Only, the place shown last after a restart.
add_test(NAME app.ui-windows-open-from-project
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-open-from-project-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-open-from-project"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-open-from-project PROPERTIES TIMEOUT 300)
