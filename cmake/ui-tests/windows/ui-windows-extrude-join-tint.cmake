# SPDX-License-Identifier: MIT
# An Extrude Join's target body in the command's look (mitcad#2069): the
# target tinted faintly and the added material blue. The Linux test's
# shared input script (tools/ui-extrude-join-tint-test.sh).
add_test(NAME app.ui-windows-extrude-join-tint
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-join-tint-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-join-tint"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-join-tint PROPERTIES TIMEOUT 300)
