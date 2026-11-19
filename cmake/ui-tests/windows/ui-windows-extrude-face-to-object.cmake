# SPDX-License-Identifier: MIT
# Extrude of a body face up to another face of the same body (mitcad#2033):
# the lower step's top extruded To Object onto the higher step's top and
# joined, one level body. The Linux test's shared input script
# (tools/ui-extrude-face-to-object-test.sh).
add_test(NAME app.ui-windows-extrude-face-to-object
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-face-to-object-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-face-to-object"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-face-to-object PROPERTIES TIMEOUT 300)
