# SPDX-License-Identifier: MIT
# The render worker while a section analysis is shown and hidden in the
# rendered view (mitcad#1432): twelve times while Cycles traces on the
# CPU, the worker never stopping. The Linux test's shared input script
# (tools/ui-render-section-toggle-test.sh).
add_test(NAME app.ui-windows-render-section-toggle
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-render-section-toggle-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-render-section-toggle"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-render-section-toggle PROPERTIES TIMEOUT 420)
