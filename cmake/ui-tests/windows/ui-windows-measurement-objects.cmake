# SPDX-License-Identifier: MIT
# Measurement objects (mitcad#649): kept from the Measure panel and its
# history (annotations since mitcad#702), listed in the browser's
# Annotations folder, a 4 x transparent snapshot, saved and opened
# again.
add_test(NAME app.ui-windows-measurement-objects
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measurement-objects-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-measurement-objects"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-measurement-objects PROPERTIES TIMEOUT 300)
