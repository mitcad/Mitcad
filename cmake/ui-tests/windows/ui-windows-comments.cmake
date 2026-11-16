# SPDX-License-Identifier: MIT
# Comments for AI agents (mitcad#1119): Add Comment on a face, its pin
# and its row in the Comments panel, addressed by an agent through the
# live MCP bridge, resolved in the panel, the pins hidden and shown,
# opened again in the Floating layout.
add_test(NAME app.ui-windows-comments
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-comments-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-comments" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-comments PROPERTIES TIMEOUT 300)
