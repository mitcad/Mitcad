# SPDX-License-Identifier: MIT
# The selection's look (mitcad#1803): a selected edge's highlight
# Selected line width's pixels times the device pixel ratio wide, at
# ratio 1 and 1.25 (2 leaves the 1024 x 768 screen's Docked window no
# view), in Preferences › Display › Selection color; a change of
# either draws the selection anew, in the view and in sketch mode. The
# Linux test's shared input script (tools/ui-selection-look-test.sh).
add_test(NAME app.ui-windows-selection-look
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-selection-look-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-selection-look"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-selection-look PROPERTIES TIMEOUT 600)
