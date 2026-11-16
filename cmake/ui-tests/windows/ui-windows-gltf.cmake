# SPDX-License-Identifier: MIT
# glTF export and import (mitcad#1826): File > Export to .glb and .gltf,
# read back by MitCad's own reader; File > Import and Open of both. The
# Linux test's shared input scripts (tools/ui-gltf-test.sh).
add_test(NAME app.ui-windows-gltf
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-gltf-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-gltf"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-gltf PROPERTIES TIMEOUT 300)
