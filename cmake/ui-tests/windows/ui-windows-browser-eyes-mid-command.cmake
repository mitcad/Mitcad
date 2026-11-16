# SPDX-License-Identifier: MIT
# Browser eyes while a command is open (mitcad#1263): Section1's eye
# during a modelled Thread clips its preview and OK adds the thread (the
# eye an undo step of its own before it); Esc after the eye keeps the
# section; the bodies' eyes during Fillet keep its selection; the Linux
# test's shared input script.
add_test(NAME app.ui-windows-browser-eyes-mid-command
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-eyes-mid-command-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-eyes-mid-command"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-eyes-mid-command PROPERTIES TIMEOUT 600)
