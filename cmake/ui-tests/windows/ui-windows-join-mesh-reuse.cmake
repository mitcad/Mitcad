# SPDX-License-Identifier: MIT
# A Join made in the Extrude panel on a plate's end: every face of the
# joined body meshed where the view draws it, the plate's top (its mesh
# taken from the plate as shown) pickable (GitHub #126, mitcad#1243);
# the Linux test's shared input script.
add_test(NAME app.ui-windows-join-mesh-reuse
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-join-mesh-reuse-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-join-mesh-reuse"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-join-mesh-reuse PROPERTIES TIMEOUT 240)
