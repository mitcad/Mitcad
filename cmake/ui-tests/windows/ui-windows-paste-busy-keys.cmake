# SPDX-License-Identifier: MIT
# Keys while a pasted component is still computed (mitcad#1360, GitHub
# mitcad/Mitcad#156, a crash of 0.47.0 here): Paste New opens
# Move/Copy, and Shift and Up in its X Distance field while the preview
# computes neither read the document nor end the application; OK places
# the copy once the job has ended. The Linux test's shared input script.
add_test(NAME app.ui-windows-paste-busy-keys
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-paste-busy-keys-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-paste-busy-keys"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-paste-busy-keys PROPERTIES TIMEOUT 240)
