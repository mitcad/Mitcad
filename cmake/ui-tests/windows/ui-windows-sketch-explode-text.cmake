# SPDX-License-Identifier: MIT
# Explode Text (mitcad#2163): a text right-clicked, Explode Text from its
# context menu, then Trim on a piece of the letter. The Linux test's shared
# input script (tools/ui-sketch-explode-text-test.sh).
add_test(NAME app.ui-windows-sketch-explode-text
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-explode-text-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-explode-text"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-explode-text PROPERTIES TIMEOUT 200)
