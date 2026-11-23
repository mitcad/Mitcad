# SPDX-License-Identifier: MIT
# Fillet's Full Round (mitcad#2279): choosing Full Round hides Radius, the
# top face picked into Edges is refused with a message saying to pick the
# face's two edges, and its two edges build the full round. The Linux
# test's shared input script (tools/ui-fillet-full-round-test.sh).
add_test(NAME app.ui-windows-fillet-full-round
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-full-round-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-full-round"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fillet-full-round PROPERTIES TIMEOUT 300)
