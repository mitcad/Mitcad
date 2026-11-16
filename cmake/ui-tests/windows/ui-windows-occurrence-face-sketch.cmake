# SPDX-License-Identifier: MIT
# A sketch on a face of an occurrence turned and moved (mitcad#505):
# its frame in the design on the face, Look At normal to it and a drawn
# line on the face, with the component active and with the root.
add_test(NAME app.ui-windows-occurrence-face-sketch
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-occurrence-face-sketch-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-occurrence-face-sketch"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-occurrence-face-sketch PROPERTIES TIMEOUT 300)
