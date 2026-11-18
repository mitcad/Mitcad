# SPDX-License-Identifier: MIT
# A comment thread with an agent (mitcad#2010): Add Comment with Whole
# Design in an empty design, an agent's long reply in full in the Comments
# panel's details pane, waiting for your answer, and Reply… showing the
# reply it answers; the Linux test's shared input script.
add_test(NAME app.ui-windows-comments-thread
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-comments-thread-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-comments-thread"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-comments-thread PROPERTIES TIMEOUT 300)
