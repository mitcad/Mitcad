# SPDX-License-Identifier: MIT
# Suppress Features asks about the features that use it: Cancel, Suppress
# Only This, Suppress All with one Undo, Unsuppress without a question
# (mitcad#2314); the selection rule in the timeline and on the browser's
# feature rows (mitcad#2376).
add_test(NAME app.ui-windows-suppress-dependents
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-suppress-dependents-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-suppress-dependents"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-suppress-dependents PROPERTIES TIMEOUT 480)
