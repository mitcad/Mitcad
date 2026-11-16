# SPDX-License-Identifier: MIT
# The BUILD tab (mitcad#375): a library of synthetic parts fetched from
# a local git repository; a frame of four profiles (a mitred end, a
# bound length), a connector, a panel, a rail with a carriage; saved,
# reopened, the cut list's CSV and PDF; the part picker (mitcad#418).
add_test(NAME app.ui-windows-build
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-build"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build PROPERTIES TIMEOUT 900)
