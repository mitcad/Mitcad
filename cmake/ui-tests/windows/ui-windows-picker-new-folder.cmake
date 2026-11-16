# SPDX-License-Identifier: MIT
# The project picker's New Folder (mitcad#1649): a folder made in the
# selected project and named in place, the same name refused, the
# design saved into the new folder; a folder made in a cancelled
# picker stays. The Linux test's shared input script
# (tools/ui-picker-new-folder-test.sh).
add_test(NAME app.ui-windows-picker-new-folder
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-picker-new-folder-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-picker-new-folder"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-picker-new-folder PROPERTIES TIMEOUT 300)
