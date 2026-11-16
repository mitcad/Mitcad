# SPDX-License-Identifier: MIT
# The Material and Appearance tools (mitcad#1059): a library material
# chosen in the picker for two bodies, OK and Ctrl+Z; a component's
# default material; the fx bound to an option; the Appearance tool's
# picker on a body and on a face; the windows without Assign; the
# pickers' Edit Materials… and Edit Appearances… (mitcad#1415).
add_test(NAME app.ui-windows-material-tool
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-material-tool-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-material-tool"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-material-tool PROPERTIES TIMEOUT 480)
