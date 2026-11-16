# SPDX-License-Identifier: MIT
# The message band and the message history (mitcad#1042): an error in
# the band above the timeline stays until it is closed or the next
# command starts, a hint fades to the stub, the history's row of a
# feature selects it; the Docked status bar's history button.
add_test(NAME app.ui-windows-message-band
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-message-band-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-message-band"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-message-band PROPERTIES TIMEOUT 300)
