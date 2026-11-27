# SPDX-License-Identifier: MIT
# Point pairs and the crop box of the point cloud tools (mitcad#2565): the
# shared steps of tools/ui-scripts/point-cloud-pairs.mitcad-ui, a scan
# aligned onto a block by three point pairs clicked in the view and the
# best fit, then cropped by a box (tools/ui-point-cloud-pairs-test.sh).
add_test(NAME app.ui-windows-point-cloud-pairs
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-point-cloud-pairs-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-point-cloud-pairs"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-point-cloud-pairs PROPERTIES TIMEOUT 900)
