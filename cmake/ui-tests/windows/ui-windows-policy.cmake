# SPDX-License-Identifier: MIT
# Design policy (mitcad#220): the new design's Component1, a root that
# refuses sketches, the policy dialog and its presets, undo, and
# sketch planes of other components left out of picking.
add_test(NAME app.ui-windows-policy
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-policy-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-policy"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-policy PROPERTIES TIMEOUT 300)
