# SPDX-License-Identifier: MIT
# Open Cage as Form (mitcad#1757, mitcad#1810): an imported body that
# keeps a control cage (a synthetic cube net given to a design made by
# mitcad-cli) opened from its Browser row as a Form in Edit Form, a face
# moved, Close keeping it. The Linux test's shared input script
# (tools/ui-form-kept-cage-test.sh).
add_test(NAME app.ui-windows-form-kept-cage
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-form-kept-cage-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-form-kept-cage"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-form-kept-cage PROPERTIES TIMEOUT 300)
