# SPDX-License-Identifier: MIT
# Live MCP for debugging (mitcad#265): the actual application and
# mitcad-cli.exe mcp --connect; queries and sketch commands with a
# sketch open, ui_state, ui_select, ui_screenshot, ui_command, Save,
# ui_log.
add_test(NAME app.ui-windows-mcp
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mcp-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-mcp"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-mcp PROPERTIES TIMEOUT 300)
