# SPDX-License-Identifier: MIT
# CAM mill-turn: live tools on the sub-spindle, the B head's and the
# sub-spindle's codes in Machines and Tools..., the sub-spindle's travel in
# the lathe check (mitcad#2375): Edit Machine... with the sub-spindle's
# page, the back faced, drilled and milled on the sub-spindle, Generate,
# Simulate, Post Process... to the Fanuc turning dialect and the lathe
# check's finding. The Linux test's shared input script
# (tools/ui-cam-mill-turn-sub-test.sh). A second Simulate draws the
# sub-spindle where it waits (mitcad#2405): a longer timeout.
add_test(NAME app.ui-windows-cam-mill-turn-sub
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cam-mill-turn-sub-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cam-mill-turn-sub"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cam-mill-turn-sub PROPERTIES TIMEOUT 700)
