# SPDX-License-Identifier: MIT
# A fillet's short tangent chain is a remark, not a warning
# (mitcad#1914): the accepted feature marks nothing on the Timeline,
# and its panel points the short chain out as information while it is
# edited. On the reported design of the private test files
# (MITCAD_MITCAD_REPORTED); skipped without it. The Linux test's shared
# input script (tools/ui-fillet-chain-remark-test.sh).
add_test(NAME app.ui-windows-fillet-chain-remark
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-chain-remark-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-chain-remark"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fillet-chain-remark PROPERTIES
  TIMEOUT 1500 SKIP_RETURN_CODE 77)
