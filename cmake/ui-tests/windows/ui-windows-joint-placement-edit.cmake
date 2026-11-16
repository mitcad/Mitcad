# SPDX-License-Identifier: MIT
# A BUILD part's joint and placement edited (mitcad#1885, GitHub
# mitcad/Mitcad#354): a joint onto a profile's mount frame with a frame
# override opens in Joint and keeps the override; Placement changes a
# side placement's From keeping the part in place, then its Offset. The
# Linux test's shared input script (tools/ui-joint-placement-edit-test.sh).
add_test(NAME app.ui-windows-joint-placement-edit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-joint-placement-edit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-joint-placement-edit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-joint-placement-edit PROPERTIES TIMEOUT 300)
