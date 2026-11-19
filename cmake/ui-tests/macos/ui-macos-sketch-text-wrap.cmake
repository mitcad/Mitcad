# SPDX-License-Identifier: MIT
# A sketch text drawn in a frame wraps to the frame's width (mitcad#2029):
# the shared steps of tools/ui-sketch-text-wrap-test.sh in the Qt frontend,
# a frame dragged with the Text tool and a long text typed, Edit Text's
# Height, a frame corner, Wrap in Frame and a line break in its Text box.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-text-wrap
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-text-wrap-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-text-wrap")
  set_tests_properties(app.ui-macos-sketch-text-wrap PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
