# SPDX-License-Identifier: MIT
# Show used sketches under their features (mitcad#1699): a Features
# folder at once with a shared sketch under each feature that uses it,
# marked shared; Edit Sketch from such a row; off, the rows as before.
# The Linux test's shared input script
# (tools/ui-browser-used-sketches-test.sh).
add_test(NAME app.ui-windows-browser-used-sketches
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-used-sketches-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-used-sketches"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-used-sketches PROPERTIES TIMEOUT 300)
