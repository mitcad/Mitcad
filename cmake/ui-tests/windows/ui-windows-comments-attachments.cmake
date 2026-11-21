# SPDX-License-Identifier: MIT
# Images on comments and replies (mitcad#2011): Capture and Annotate… from
# Add Comment's panel, Reply… with Attach Image…, the thumbnails and the
# viewer, kept with the design; the Linux test's shared input scripts
# (tools/ui-comments-attachments-test.sh).
add_test(NAME app.ui-windows-comments-attachments
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-comments-attachments-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-comments-attachments"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-comments-attachments PROPERTIES TIMEOUT 600)
