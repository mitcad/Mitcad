# SPDX-License-Identifier: MIT
# Minimum Distance (mitcad#2519): Body1 to Body2 picked in the browser,
# 5 mm, closer than a 6 mm clearance and clear of a 4 mm one; kept as
# MinDistance1, measured again when the blocks' parameter changes, hidden,
# shown, edited, saved, deleted and undone. The Linux test's shared input
# script (tools/ui-minimum-distance-test.sh).
add_test(NAME app.ui-windows-minimum-distance
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-minimum-distance-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-minimum-distance"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-minimum-distance PROPERTIES TIMEOUT 360)
