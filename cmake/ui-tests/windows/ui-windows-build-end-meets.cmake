# SPDX-License-Identifier: MIT
# Mitering two BUILD profiles placed apart (mitcad#1940): the bound-length
# refusal says how to miter a corner; Ends labels the ends A and B, a click
# near the corner chooses the end, and Also the Component It Meets makes the
# shared miter. The Linux test's shared input script
# (tools/ui-build-end-meets-test.sh).
add_test(NAME app.ui-windows-build-end-meets
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-end-meets-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-end-meets"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-end-meets PROPERTIES TIMEOUT 300)
