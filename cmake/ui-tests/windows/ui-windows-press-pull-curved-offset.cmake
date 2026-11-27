# SPDX-License-Identifier: MIT
# Press Pull's Offset on a curved face (mitcad#2509): on a tapered
# Extrude's side the arrow starts on the face along its outward normal;
# dragged and typed, the same offset; 0.5 mm typed, the grown frustum. The
# Linux test's shared input script
# (tools/ui-press-pull-curved-offset-test.sh).
add_test(NAME app.ui-windows-press-pull-curved-offset
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-press-pull-curved-offset-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-press-pull-curved-offset"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-press-pull-curved-offset PROPERTIES TIMEOUT 300)
