# SPDX-License-Identifier: MIT
# A sketch on a planar face starts with the face's outline, and Convert to
# Sketch Geometry takes a profile (mitcad#2051): the steps
# tools/ui-scripts/sketch-face-outline.mitcad-ui shares with the Linux test
# and app.ui-windows-sketch-face-outline, in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-face-outline
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-face-outline-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-sketch-face-outline")
  set_tests_properties(app.ui-macos-sketch-face-outline PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
