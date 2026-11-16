# SPDX-License-Identifier: MIT
# Form and Edit Form (mitcad#1757): a box cage's face picked in the
# view, moved and extruded, an edge creased and symmetry set, each one
# undo step whose body is a solid. The Linux test's shared input
# script (tools/ui-form-test.sh).
add_test(NAME app.ui-windows-form
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-form-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-form"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-form PROPERTIES TIMEOUT 480)
