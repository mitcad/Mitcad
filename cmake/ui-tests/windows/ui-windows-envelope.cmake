# SPDX-License-Identifier: MIT
# A linked component shown by its envelope (mitcad#1271): Show as
# Envelope... at 20 mm in a linked machine's browser menu, one body
# shown instead of three; Show in Full, the three again; undo.
add_test(NAME app.ui-windows-envelope
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-envelope-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-envelope"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-envelope PROPERTIES TIMEOUT 300)
