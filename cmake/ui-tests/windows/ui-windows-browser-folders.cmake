# SPDX-License-Identifier: MIT
# Folders in the Browser (mitcad#2103): components and bodies into folders
# from the row menu and by dragging, the folder's eye, Rename, Ungroup and
# Undo, kept with the design. The Linux test's shared input script
# (tools/ui-browser-folders-test.sh).
add_test(NAME app.ui-windows-browser-folders
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-folders-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-folders"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-folders PROPERTIES TIMEOUT 300)
