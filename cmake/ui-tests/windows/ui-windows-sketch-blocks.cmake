# SPDX-License-Identifier: MIT
# Sketch blocks (mitcad#2110): Create Block of a slot, two instances
# inserted, one placed by a coincident and an angle, the block edited in
# place and every instance following, one exploded. The Linux test's
# shared input script (tools/ui-sketch-blocks-test.sh).
add_test(NAME app.ui-windows-sketch-blocks
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-blocks-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-blocks"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-blocks PROPERTIES TIMEOUT 200)
