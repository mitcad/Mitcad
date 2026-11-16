# SPDX-License-Identifier: MIT
# Construction planes drawn at the size and shape of what they come
# from (mitcad#1827): a plane saved with a size opens unchanged, one off
# a long narrow face is a rectangle over it, one over a small face is
# small, a handle's drag stores a size and Reset Size clears it. The
# Linux test's shared input script (tools/ui-construction-size-test.sh).
add_test(NAME app.ui-windows-construction-size
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-construction-size-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-construction-size"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-construction-size PROPERTIES TIMEOUT 300)
