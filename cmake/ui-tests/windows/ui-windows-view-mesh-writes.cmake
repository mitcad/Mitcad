# SPDX-License-Identifier: MIT
# The view never meshes the model's shapes itself (mitcad#1553,
# mitcad#1586): picks while the background evaluation runs leave the
# body's edges unmeshed, and the construction planes, sketch profiles
# and curves the view meshes are shapes of its own. The Linux test's
# shared input script (tools/ui-view-mesh-writes-test.sh).
add_test(NAME app.ui-windows-view-mesh-writes
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-mesh-writes-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-view-mesh-writes"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-mesh-writes PROPERTIES TIMEOUT 420)
