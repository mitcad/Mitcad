# SPDX-License-Identifier: MIT
# Convert to Features (mitcad#1815): a timber member from a library made in
# the test, joined to a plate, converted from its row's menu; its sketch
# and extrusion in the timeline, the section's width changed, the joint
# holding. The shared steps of tools/ui-convert-features-test.sh and
# tools/ui-windows-convert-features-test.ps1 in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-convert-features
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-convert-features-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-convert-features")
  set_tests_properties(app.ui-macos-convert-features PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
