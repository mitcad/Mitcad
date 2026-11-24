# SPDX-License-Identifier: MIT
# Split Body with a face of another component as the Splitting Tool
# (mitcad#2299): the shared steps of
# tools/ui-scripts/split-body-other-component.mitcad-ui, B's block split by
# a face of A's block picked where A:1 shows it, undone, edited, and the cut
# following A:1 (tools/ui-split-body-other-component-test.sh).
add_test(NAME app.ui-windows-split-body-other-component
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-split-body-other-component-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-split-body-other-component"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-split-body-other-component PROPERTIES TIMEOUT 600)
