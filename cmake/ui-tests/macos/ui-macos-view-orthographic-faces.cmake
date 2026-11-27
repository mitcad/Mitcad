# SPDX-License-Identifier: MIT
# Perspective with Orthographic Faces (mitcad#2473): the shared steps of
# tools/ui-view-orthographic-faces-test.sh in the Qt frontend: a cube face
# click logs an orthographic camera, an orbit a perspective one, the cube's
# edge and corner stay perspective.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-view-orthographic-faces
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-view-orthographic-faces-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-view-orthographic-faces")
  set_tests_properties(app.ui-macos-view-orthographic-faces PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
