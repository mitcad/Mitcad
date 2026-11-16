# SPDX-License-Identifier: MIT
# The part of GitHub mitcad/Mitcad#173 (mitcad#1422) built through the
# UI (a slot, a circle on its end, two extrusions) and the plate's top
# edge rounded into the boss with Tangent Chain: a failing radius's error
# in the panel, 3 mm valid; then a fillet crashing inside OCCT
# (MITCAD_TEST_OCCT_CRASH=fillet) is an error and the application goes
# on. The Linux test's shared input scripts
# (tools/ui-fillet-boss-tangent-test.sh).
add_test(NAME app.ui-windows-fillet-boss-tangent
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-boss-tangent-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-boss-tangent"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-fillet-boss-tangent PROPERTIES TIMEOUT 600)
