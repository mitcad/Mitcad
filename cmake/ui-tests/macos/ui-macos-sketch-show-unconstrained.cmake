# SPDX-License-Identifier: MIT
# Show Unconstrained (mitcad#2026): the shared steps of
# tools/ui-sketch-show-unconstrained-test.sh in the Qt frontend, the Sketch
# Palette's button, a click on the degrees of freedom badge and the
# sketch's context menu each selecting the free line and its two points.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-show-unconstrained
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-show-unconstrained-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-show-unconstrained")
  set_tests_properties(app.ui-macos-sketch-show-unconstrained PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
