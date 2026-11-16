# SPDX-License-Identifier: MIT
# Extrude's automatic operation (mitcad#289): Join on a body's face,
# Cut into it and Join out again, New Body away from bodies, a hand
# picked operation that stays, and an edit that keeps its operation.
add_test(NAME app.ui-windows-auto-operation
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-auto-operation-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-auto-operation"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-auto-operation PROPERTIES TIMEOUT 300)
