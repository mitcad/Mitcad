# SPDX-License-Identifier: MIT
# Extrude (E) from a zoomed sketch on a face: the sketch and its body in
# the free area (mitcad#1227), a drawn profile rather than the face's
# leftover region (mitcad#1226); the Linux test's shared input script.
add_test(NAME app.ui-windows-extrude-from-sketch
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-from-sketch-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-from-sketch"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-from-sketch PROPERTIES TIMEOUT 240)
