# SPDX-License-Identifier: MIT
# The Boost development button (mitcad#1990): shown in the Floating top
# bar and the Docked toolbar, a click logs the Ko-fi link (no browser), its
# × hides it and saves it, Preferences › Display shows it again. The Linux
# test's shared input script (tools/ui-boost-button-test.sh).
add_test(NAME app.ui-windows-boost-button
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-boost-button-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-boost-button"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-boost-button PROPERTIES TIMEOUT 300)
