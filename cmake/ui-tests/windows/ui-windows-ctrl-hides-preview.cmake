# SPDX-License-Identifier: MIT
# Ctrl held alone hides a command's preview while its inputs are picked
# (mitcad#1421): a new Extrude gains a fourth circle and Fillet1 edited
# from the timeline an edge while Ctrl hides their previews; Ctrl+Z and
# Ctrl+S show the preview again at once. The Linux test is
# tools/ui-ctrl-hides-preview-test.sh.
add_test(NAME app.ui-windows-ctrl-hides-preview
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-ctrl-hides-preview-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-ctrl-hides-preview" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-ctrl-hides-preview PROPERTIES TIMEOUT 300)
