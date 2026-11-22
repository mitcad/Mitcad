# SPDX-License-Identifier: MIT
# Groups of user parameters in Edit Parameters (mitcad#2180): a group made,
# two parameters moved into it by the context menu, the group renamed in
# place, collapsed (kept when the dialog opens again) and deleted, its
# parameters back under User Parameters. The Linux test's shared input
# script (tools/ui-parameter-groups-test.sh).
add_test(NAME app.ui-windows-parameter-groups
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-parameter-groups-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-parameter-groups"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-parameter-groups PROPERTIES TIMEOUT 300)
