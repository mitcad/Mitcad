# SPDX-License-Identifier: MIT
# Axis Perpendicular at Point on a plane (mitcad#2282): the shared steps of
# tools/ui-construct-axis-on-plane-test.sh in the Qt frontend, its Face
# field taking a construction plane and the origin plane XY.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-construct-axis-on-plane
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-construct-axis-on-plane-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-construct-axis-on-plane")
  set_tests_properties(app.ui-macos-construct-axis-on-plane PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
