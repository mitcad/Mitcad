# SPDX-License-Identifier: MIT
# The Projects panel's rows (mitcad#529): a 55-character design name
# elided in the middle in the Floating card, its lock (Editing
# elsewhere) as a mark, the full name in the tooltip; the card widened by
# its edge shows the whole name and keeps its width after a restart.
add_test(NAME app.ui-windows-projects-rows
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-projects-rows-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-projects-rows"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-projects-rows PROPERTIES TIMEOUT 300)
