# SPDX-License-Identifier: MIT
# Cone (mitcad#2239) on Mesa's software renderer: a cone placed on XY and
# made a frustum, its volumes and faces checked; the shared steps of
# tools/ui-primitive-cone-test.sh.
add_test(NAME app.ui-windows-primitive-cone
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-primitive-cone-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-primitive-cone"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-primitive-cone PROPERTIES TIMEOUT 180)
