# SPDX-License-Identifier: MIT
# Sketches on curved faces (mitcad#1768, mitcad#1853): the shared steps of
# tools/ui-sketch-cylinder-face-test.sh (a rectangle typed on a cylinder's
# side) and tools/ui-sketch-cone-face-test.sh (one on a cone's side, no
# layout grid on the face, the dimensions' lines drawn on it, a picture in
# files/) in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  foreach(face IN ITEMS cylinder cone)
    add_test(NAME app.ui-macos-sketch-${face}-face
      COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-${face}-face-test.sh"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-${face}-face")
    set_tests_properties(app.ui-macos-sketch-${face}-face PROPERTIES
      TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
  endforeach()
endif()
