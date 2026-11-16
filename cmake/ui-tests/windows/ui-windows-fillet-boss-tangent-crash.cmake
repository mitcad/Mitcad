# SPDX-License-Identifier: MIT
# Fillet on the slot-and-boss part (mitcad#1429, GitHub mitcad/Mitcad#173):
# the plate's top edge that runs tangentially into the boss, previewed at
# 3, 6 and 1 mm, Tangent Chain off and on, the edges hovered, Enter and
# Escape; the application stays up whatever the fillet gives.
add_test(NAME app.ui-windows-fillet-boss-tangent-crash
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-boss-tangent-crash-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-boss-tangent-crash"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fillet-boss-tangent-crash PROPERTIES TIMEOUT 600)
