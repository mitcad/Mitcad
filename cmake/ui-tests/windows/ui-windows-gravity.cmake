# SPDX-License-Identifier: MIT
# Gravity and a hanging cable (mitcad#1713): gravity from a plate's top
# face, a hose let hang from its ends onto a plank, its middle node
# pinned and let go, dragged and let go, kept with Enter and undone, and
# put back with Esc, in the Docked and the Floating layout; the Linux
# test's shared input script (tools/ui-gravity-test.sh).
add_test(NAME app.ui-windows-gravity
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-gravity-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-gravity"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-gravity PROPERTIES TIMEOUT 700)
