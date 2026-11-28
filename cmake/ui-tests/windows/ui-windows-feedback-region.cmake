# SPDX-License-Identifier: MIT
# Send Feedback on a Region (mitcad#2616): F9 and a drag on the overlay, the
# form with the region's screenshot, Esc and a click cancel, F9 held. The
# Linux test's shared input script (tools/ui-feedback-region-test.sh).
add_test(NAME app.ui-windows-feedback-region
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-feedback-region-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-feedback-region"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-feedback-region PROPERTIES TIMEOUT 300)
