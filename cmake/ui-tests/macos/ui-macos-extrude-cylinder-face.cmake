# SPDX-License-Identifier: MIT
# Extrude of a sketch on a cylinder's side with the automatic operation
# (mitcad#1908, GitHub mitcad/Mitcad#362): the shared steps of
# tools/ui-extrude-cylinder-face-test.sh in the Qt frontend, the picked
# profiles highlighted on the face, the distance arrow starting there, the
# automatic Join built and added.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-extrude-cylinder-face
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-extrude-cylinder-face-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-extrude-cylinder-face")
  set_tests_properties(app.ui-macos-extrude-cylinder-face PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
