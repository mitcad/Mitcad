# SPDX-License-Identifier: MIT
# Picks on clipped bodies (mitcad#233): Project with Hide Above Sketch
# on, Fillet under Section Analysis, by the bodies' own names.
add_test(NAME app.ui-windows-section-pick
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-pick-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-section-pick"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-section-pick PROPERTIES TIMEOUT 300)
