# SPDX-License-Identifier: MIT
# One update for the whole application (mitcad#1890): with the update
# tests' fake offer, Install in the first design's window, a second
# design's row shows the download, the first design closed leaves it
# going on in the second to "downloaded"; Exit during a download asks
# and Keep Downloading keeps MitCad open. The fake offer is the test
# driver's step update-fake-offer, so the dev preset runs it all. The
# Linux test's shared input script
# (tools/ui-update-one-row-test.sh).
add_test(NAME app.ui-windows-update-one-row
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-update-one-row-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-update-one-row"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-update-one-row PROPERTIES TIMEOUT 360)
