# SPDX-License-Identifier: MIT
# Plane Perpendicular to Face at Point (mitcad#2111): the shared steps of
# tools/ui-construct-perpendicular-plane-test.sh in the Qt frontend, offered
# by the view's context menu for a face and a point, turned by its Angle,
# and a sketch on it taking its frame.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-construct-perpendicular-plane
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-construct-perpendicular-plane-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-construct-perpendicular-plane")
  set_tests_properties(app.ui-macos-construct-perpendicular-plane PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
