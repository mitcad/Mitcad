# SPDX-License-Identifier: MIT
# A Flange's profile sketch edited from its panel (mitcad#1648): Edit
# Profile makes the sketch on the wall and opens it while the panel
# waits, Finish Sketch brings the panel back, OK adds the flange with
# its profile. The Linux test's shared input script
# (tools/ui-sheet-flange-profile-test.sh).
add_test(NAME app.ui-windows-sheet-flange-profile
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-flange-profile-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-flange-profile"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-flange-profile PROPERTIES TIMEOUT 300)
