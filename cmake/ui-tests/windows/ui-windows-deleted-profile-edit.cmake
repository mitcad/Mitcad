# SPDX-License-Identifier: MIT
# Editing an extrusion whose profile was deleted from its sketch: the
# missing profile is left out and a new one picked (mitcad#1264).
add_test(NAME app.ui-windows-deleted-profile-edit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-deleted-profile-edit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-deleted-profile-edit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-deleted-profile-edit PROPERTIES TIMEOUT 300)
