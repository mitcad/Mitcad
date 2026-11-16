# SPDX-License-Identifier: MIT
# .serp files (mitcad#1870): --open of the import's test file (B-reps
# OCCT wrote, a block placed twice) and File > Import of it into the
# design, asked where it goes. The Linux test's shared input script
# (tools/ui-serp-test.sh).
add_test(NAME app.ui-windows-serp
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-serp-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-serp"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-serp PROPERTIES TIMEOUT 300)
