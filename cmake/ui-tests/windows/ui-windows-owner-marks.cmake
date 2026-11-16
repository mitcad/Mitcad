# SPDX-License-Identifier: MIT
# What is picked in the view marks its owners (mitcad#1704): a face its
# body's row and its feature in the timeline, a profile its sketch's,
# neither selected; Delete with the face selected acts on the face;
# after Collapse All the marked row is opened and scrolled to. Docked and
# Floating (tools/ui-owner-marks-test.sh).
add_test(NAME app.ui-windows-owner-marks
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-owner-marks-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-owner-marks"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-owner-marks PROPERTIES TIMEOUT 420)
