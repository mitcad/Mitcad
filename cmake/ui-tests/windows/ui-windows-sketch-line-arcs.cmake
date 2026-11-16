# SPDX-License-Identifier: MIT
# The Line tool's arcs and the Midpoint Line (mitcad#1393): a drag along
# a line from its end draws a tangent arc from it, a drag across a
# chain's line a normal arc with a typed radius dimensioned, and a
# Midpoint Line drawn with two clicks has its middle at the first. The
# Linux test is tools/ui-sketch-line-arcs-test.sh.
add_test(NAME app.ui-windows-sketch-line-arcs
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-line-arcs-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-line-arcs" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-line-arcs PROPERTIES TIMEOUT 300)
