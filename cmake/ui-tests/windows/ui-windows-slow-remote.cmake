# SPDX-License-Identifier: MIT
# A Cloud project with a slow server (mitcad#1895): its server a local
# bare repository behind an ssh that waits 5 s per connection; New
# Folder in Save to Project, Save in the new folder and Save again are
# recorded at once and sent in the background, the UI thread never
# stalls, and the server has every version. The Linux test's shared
# input script (tools/ui-slow-remote-test.sh).
add_test(NAME app.ui-windows-slow-remote
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-slow-remote-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-slow-remote"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-slow-remote PROPERTIES TIMEOUT 600)
