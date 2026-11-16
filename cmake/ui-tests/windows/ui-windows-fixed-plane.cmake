# SPDX-License-Identifier: MIT
# Features on a fixed plane (mitcad#1137): every type Place On places
# opens for editing with the plane listed "(fixed)"; a new size keeps
# the plane, a face picked in Place On replaces it.
add_test(NAME app.ui-windows-fixed-plane
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fixed-plane-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-fixed-plane"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fixed-plane PROPERTIES TIMEOUT 300)
