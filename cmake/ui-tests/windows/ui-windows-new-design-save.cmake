# SPDX-License-Identifier: MIT
# Save of a new design (mitcad#2183): with a remote project (a local git
# repository following a local bare repository), Ctrl+S on a new design
# asks On this computer or To a project; Esc saves nothing, each answer
# opens its dialog, Remember my choice keeps the answer and Preferences ›
# General puts the question back. The Linux test's shared input script
# (tools/ui-new-design-save-test.sh).
add_test(NAME app.ui-windows-new-design-save
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-new-design-save-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-new-design-save"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-new-design-save PROPERTIES TIMEOUT 300)
