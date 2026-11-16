# SPDX-License-Identifier: MIT
# A construction plane of the active component on a face of another
# component's turned occurrence (mitcad#1029): kept in the active one,
# linked, its preview where the finished plane is shown.
add_test(NAME app.ui-windows-construct-occurrence
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-construct-occurrence-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-construct-occurrence"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-construct-occurrence PROPERTIES TIMEOUT 300)
