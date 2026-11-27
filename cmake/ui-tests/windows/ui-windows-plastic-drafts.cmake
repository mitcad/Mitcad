# SPDX-License-Identifier: MIT
# Plastic Feature's fields of mitcad#2511: a lip with Follow Tangency and a
# Side Draft of its own, a boss with Rib Draft and a rib up to the wall,
# reopened with them, of the analytic volumes. The Linux test's shared input
# script (tools/ui-plastic-drafts-test.sh).
add_test(NAME app.ui-windows-plastic-drafts
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-plastic-drafts-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-plastic-drafts"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-plastic-drafts PROPERTIES TIMEOUT 300)
