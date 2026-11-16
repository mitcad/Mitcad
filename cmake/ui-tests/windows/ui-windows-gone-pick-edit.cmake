# SPDX-License-Identifier: MIT
# Editing a thread whose first face is gone: the Faces chip says one is
# missing, the message names the input, and removing the gone pick lets
# the edit build (mitcad#1780). The Linux test's shared input script
# (tools/ui-gone-pick-edit-test.sh).
add_test(NAME app.ui-windows-gone-pick-edit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-gone-pick-edit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-gone-pick-edit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-gone-pick-edit PROPERTIES TIMEOUT 300)
