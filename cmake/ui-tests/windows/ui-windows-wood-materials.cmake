# SPDX-License-Identifier: MIT
# The bundled dataset's woods and panels in the class Wood
# (mitcad#1552): "oak" found in Material Dataset in the class Wood; the
# built-in woods and panels, and three plywoods made at once into the
# Materials window's Wood group (mitcad#1706). The Linux test's
# shared input script (tools/ui-wood-materials-test.sh).
add_test(NAME app.ui-windows-wood-materials
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-wood-materials-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-wood-materials"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-wood-materials PROPERTIES TIMEOUT 300)
