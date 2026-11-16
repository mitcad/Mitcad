# SPDX-License-Identifier: MIT
# The Browser's Rows background in the Floating layout (mitcad#419): a
# click beside the rows picks the plate behind them, a middle drag there
# orbits, a row still takes its click; Card again takes the click.
add_test(NAME app.ui-windows-browser-rows
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-rows-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-rows"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-rows PROPERTIES TIMEOUT 300)
