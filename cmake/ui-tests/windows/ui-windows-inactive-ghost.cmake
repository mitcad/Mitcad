# SPDX-License-Identifier: MIT
# Everything outside the active component as ghosts (mitcad#420): off
# in the settings none; on, a ghost's face picked for a sketch and a
# projection; the root activated, undone, redone; the active component
# first in picks, through a ghost in front of it.
add_test(NAME app.ui-windows-inactive-ghost
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-inactive-ghost-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-inactive-ghost"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-inactive-ghost PROPERTIES TIMEOUT 300)
