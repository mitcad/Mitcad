# SPDX-License-Identifier: MIT
# New Project's Create as Local for Now (mitcad#1601): in a folder with
# a file and a Cloud address that is not reached, the project is made
# in the new subfolder, not in the folder itself; the test's folders are
# outside any git checkout (mitcad#1574). The Linux test's shared input
# script (tools/ui-project-local-subfolder-test.sh).
add_test(NAME app.ui-windows-project-local-subfolder
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-project-local-subfolder-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-project-local-subfolder"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-project-local-subfolder PROPERTIES TIMEOUT 300)
