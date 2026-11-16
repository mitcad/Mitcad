# SPDX-License-Identifier: MIT
# Purge volumes (mitcad#705): 3D Print's Purge volumes row on two
# bodies of a white and a black filament, the table opened, a cell
# typed (set by hand, saved in the design) and reset to the computed
# volume.
add_test(NAME app.ui-windows-purge-volumes
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-purge-volumes-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-purge-volumes" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-purge-volumes PROPERTIES TIMEOUT 300)
