# SPDX-License-Identifier: MIT
# An open spline clicked in Extrude without changing Type (mitcad#2594):
# the shared steps of tools/ui-extrude-thin-open-curve-test.sh in the Qt
# frontend, the click setting Type to Thin Extrude and taking the spline,
# and the wall along it added.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-extrude-thin-open-curve
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-extrude-thin-open-curve-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-extrude-thin-open-curve")
  set_tests_properties(app.ui-macos-extrude-thin-open-curve PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
