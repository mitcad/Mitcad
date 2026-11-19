# SPDX-License-Identifier: MIT
# Kerf Bend (mitcad#1939) (mitcad#1939): the Linux test's shared input script
# (tools/ui-kerf-bend-test.sh, tools/ui-scripts/kerf-bend.mitcad-ui).
add_test(NAME app.ui-windows-kerf-bend
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-kerf-bend-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-kerf-bend"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-kerf-bend PROPERTIES TIMEOUT 240)
