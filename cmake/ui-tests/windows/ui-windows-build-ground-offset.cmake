# SPDX-License-Identifier: MIT
# A BUILD profile placed off a face by Offset and Rotate (mitcad#2046): on
# a leg's bottom face Offset 20 mm and Rotate 30 deg are previewed and
# placed, then Placement moves it to Offset 50 mm and Rotate 45 deg in one
# undo step. The Linux test's shared input script
# (tools/ui-build-ground-offset-test.sh).
add_test(NAME app.ui-windows-build-ground-offset
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-ground-offset-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-ground-offset"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-ground-offset PROPERTIES TIMEOUT 300)
