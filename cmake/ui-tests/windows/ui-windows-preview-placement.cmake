# SPDX-License-Identifier: MIT
# Extrude's preview in components placed off the origin (mitcad#266):
# hidden bodies and occurrences stay hidden, shown and new bodies are
# where their occurrences put them, a linked component's too.
add_test(NAME app.ui-windows-preview-placement
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preview-placement-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-preview-placement"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-preview-placement PROPERTIES TIMEOUT 300)
