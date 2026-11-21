# SPDX-License-Identifier: MIT
# A drawing's dimensioning standard (mitcad#2047): Standard ASME in the
# sheet's panel writes a vertical value horizontal and undoes in one step;
# Create Drawing's ANSI template sets ASME and the choice is kept. The
# Linux test's shared input script (tools/ui-drawing-standard-test.sh).
add_test(NAME app.ui-windows-drawing-standard
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-standard-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-standard"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-standard PROPERTIES TIMEOUT 180)
