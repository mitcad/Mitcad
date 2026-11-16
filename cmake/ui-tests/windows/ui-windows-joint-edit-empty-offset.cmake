# SPDX-License-Identifier: MIT
# A joint edited with its Offset emptied while typing (mitcad#1860,
# GitHub mitcad/Mitcad#347): the joint and the rigid group after it
# held in the edit's preview, nothing moved while the Offset is empty,
# OK refused, a new value committed. The Linux test's shared input
# script (tools/ui-joint-edit-empty-offset-test.sh).
add_test(NAME app.ui-windows-joint-edit-empty-offset
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-joint-edit-empty-offset-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-joint-edit-empty-offset"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-joint-edit-empty-offset PROPERTIES TIMEOUT 300)
