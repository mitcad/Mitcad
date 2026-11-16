# SPDX-License-Identifier: MIT
# The file dialogs' folders per group (mitcad#498): Import, Export and
# Open each start in their own last folder, also after a restart.
add_test(NAME app.ui-windows-dialog-folders
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-dialog-folders-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-dialog-folders"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-dialog-folders PROPERTIES TIMEOUT 300)
