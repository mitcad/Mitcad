# SPDX-License-Identifier: MIT
# A project's other files (mitcad#2204) in the Projects panel, Floating
# and Docked: a PNG and a PDF beside a design listed with their marks, Open
# handed to the system's app (a stub), Record Changes, Add Files... and a
# drop from the file manager, a rename and a delete, each one version. The
# Linux test's shared input script (tools/ui-project-files-test.sh).
add_test(NAME app.ui-windows-project-files
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-project-files-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-project-files"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-project-files PROPERTIES TIMEOUT 600)
