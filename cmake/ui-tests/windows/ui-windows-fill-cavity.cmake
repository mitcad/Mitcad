# SPDX-License-Identifier: MIT
# Fill Cavity (mitcad#1818): the shared steps of
# tools/ui-scripts/fill-cavity.mitcad-ui, a blind slit filled up to a
# plane (its volume added) and an open groove as the seed refused in
# words (tools/ui-fill-cavity-test.sh).
add_test(NAME app.ui-windows-fill-cavity
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fill-cavity-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-fill-cavity"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fill-cavity PROPERTIES TIMEOUT 600)
