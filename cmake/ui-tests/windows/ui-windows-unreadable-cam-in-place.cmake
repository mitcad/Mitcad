# SPDX-License-Identifier: MIT
# Edit in Place of a linked design whose CAM section this MitCad cannot
# read (mitcad#2587): an agent's save refused, Save asking first, Cancel
# leaving the linked file byte for byte as it was, Save and Drop Them
# writing it without the CAM. The shared steps of
# tools/ui-unreadable-cam-in-place-test.sh.
add_test(NAME app.ui-windows-unreadable-cam-in-place
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-unreadable-cam-in-place-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-unreadable-cam-in-place"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-unreadable-cam-in-place PROPERTIES TIMEOUT 420)
