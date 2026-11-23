# SPDX-License-Identifier: MIT
# A lathe made in Machines and Tools… (mitcad#2230): New Machine… > Lathe,
# its Edit Machine…, a turning tool and a parting tool, a lathe setup, Turn
# Facing and Generate. The Linux test's shared input script
# (tools/ui-cam-lathe-machine-test.sh).
add_test(NAME app.ui-windows-cam-lathe-machine
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cam-lathe-machine-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cam-lathe-machine"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cam-lathe-machine PROPERTIES TIMEOUT 360)
