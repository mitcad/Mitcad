# SPDX-License-Identifier: MIT
# Preferences > Export Settings and Import Settings (mitcad#1889, GitHub
# mitcad/Mitcad#357): a preference, a shortcut and a navigation bar pin
# exported, put back to their defaults and imported again; the file is
# LF text. The Linux test's shared input script
# (tools/ui-preferences-transfer-test.sh).
add_test(NAME app.ui-windows-preferences-transfer
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preferences-transfer-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-preferences-transfer"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-preferences-transfer PROPERTIES TIMEOUT 300)
