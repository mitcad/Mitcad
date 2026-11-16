# SPDX-License-Identifier: MIT
# A widening taper that closes a slot (mitcad#1851): Extrude succeeds,
# the slot filled where its walls meet, and Properties shows the body's
# volume. The Linux test's shared input script
# (tools/ui-taper-closes-slot-test.sh).
add_test(NAME app.ui-windows-taper-closes-slot
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-taper-closes-slot-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-taper-closes-slot"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-taper-closes-slot PROPERTIES TIMEOUT 300)
