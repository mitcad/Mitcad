# SPDX-License-Identifier: MIT
# The session log, the hang marker and the notice of an unexpected end
# (mitcad#1896): a few commands and a test hang of 3 s in the session's
# log file, killed; the next start's notice and Open Log Folder, its
# clean end. The Linux test's shared input scripts
# (tools/ui-session-log-test.sh).
add_test(NAME app.ui-windows-session-log
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-session-log-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-session-log"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-session-log PROPERTIES TIMEOUT 300)
