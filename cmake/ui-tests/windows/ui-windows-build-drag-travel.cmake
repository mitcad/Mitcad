# SPDX-License-Identifier: MIT
# Drags leave BUILD's placed parts at their Offset and a carriage slides
# within its travel limits (mitcad#2088): a drag of a placed profile is
# refused, Placement moves it and the part on it follows, the carriage
# stops at its limits. The Linux test's shared input script
# (tools/ui-build-drag-travel-test.sh).
add_test(NAME app.ui-windows-build-drag-travel
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-drag-travel-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-drag-travel"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-drag-travel PROPERTIES TIMEOUT 300)
