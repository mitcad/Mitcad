# SPDX-License-Identifier: MIT
# The command Repeat's own key (mitcad#2052): Space given to Repeat in
# Keyboard Shortcuts repeats Extrude in the 3D view, and not in a panel, a
# sketch, the browser or a rename; the Linux test's shared input script.
add_test(NAME app.ui-windows-repeat-key
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-repeat-key-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-repeat-key"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-repeat-key PROPERTIES TIMEOUT 240)
