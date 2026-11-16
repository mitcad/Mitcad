# SPDX-License-Identifier: MIT
# Timber's Along an edge (mitcad#1814): a member along a plate's edge,
# flush, moved by its four offsets, following the plate when it gets
# shorter; a round edge refused in words. The timber library of
# tools/ui-windows-timber-lib.ps1; the Linux test's shared input script
# (tools/ui-timber-edge-test.sh).
add_test(NAME app.ui-windows-timber-edge
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-timber-edge-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-timber-edge"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-timber-edge PROPERTIES TIMEOUT 300)
