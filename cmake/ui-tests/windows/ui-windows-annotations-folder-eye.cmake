# SPDX-License-Identifier: MIT
# The Browser's Annotations folder's eye (mitcad#2041): every annotation
# hidden and shown in the view, one undo step, kept with the design. The
# Linux test's shared input scripts (tools/ui-annotations-folder-eye-test.sh).
add_test(NAME app.ui-windows-annotations-folder-eye
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-annotations-folder-eye-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-annotations-folder-eye"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-annotations-folder-eye PROPERTIES TIMEOUT 300)
