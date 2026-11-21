# SPDX-License-Identifier: MIT
# The Controller list of Machines and Tools… > Edit Machine (mitcad#2119):
# a router's has the milling dialects only, a lathe's the lathe dialects
# only. The Linux test's shared input script
# (tools/ui-cam-machine-controllers-test.sh).
add_test(NAME app.ui-windows-cam-machine-controllers
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cam-machine-controllers-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cam-machine-controllers"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cam-machine-controllers PROPERTIES TIMEOUT 240)
