# SPDX-License-Identifier: MIT
# Combine with the same body in two occurrences as tools (mitcad#544):
# Part's block picked where Part:1 and where Part:2 show it, listed
# "Part:1, Part:2", both cut from the root's block; the design file
# keeps a tool in each occurrence and an edit takes both back. Extrude
# To Object to a face of another component (mitcad#762), linked.
add_test(NAME app.ui-windows-component-combine
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-component-combine-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-component-combine"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-component-combine PROPERTIES TIMEOUT 480)
