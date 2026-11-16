# SPDX-License-Identifier: MIT
# Remove Project and New Project's Create in a New Subfolder
# (mitcad#1562, GitHub mitcad/Mitcad#213): a folder with designs made a
# project in a new subfolder, which the Projects panel then removes,
# the designs kept. The Linux test's shared input script
# (tools/ui-remove-project-test.sh).
add_test(NAME app.ui-windows-remove-project
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-remove-project-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-remove-project"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-remove-project PROPERTIES TIMEOUT 300)
