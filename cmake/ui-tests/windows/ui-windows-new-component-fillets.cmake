# SPDX-License-Identifier: MIT
# An extrude edited from Join to New Component takes its fillet along
# (mitcad#2185): no feature fails, the timeline marks the extrude and the
# fillet in the new component's colour, undo and the edit back to Join
# bring the component's colour back. The Linux test's shared input script
# (tools/ui-new-component-fillets-test.sh).
add_test(NAME app.ui-windows-new-component-fillets
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-new-component-fillets-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-new-component-fillets"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-new-component-fillets PROPERTIES TIMEOUT 240)
