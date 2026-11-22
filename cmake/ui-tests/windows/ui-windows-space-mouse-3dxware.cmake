# SPDX-License-Identifier: MIT
# The 3DxWare driver (mitcad#2152, GitHub mitcad/Mitcad#529): with the
# stand-in for its runtime (MITCAD_TEST_3DXWARE), in the Floating layout,
# the design window has no native window of its own, the screen beside the
# Floating cards shows the view's background where the system draws it, and
# the driver connects ("via 3DxWare").
add_test(NAME app.ui-windows-space-mouse-3dxware
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-space-mouse-3dxware-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-space-mouse-3dxware"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-space-mouse-3dxware PROPERTIES TIMEOUT 300)
