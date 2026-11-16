# SPDX-License-Identifier: MIT
# The scenario of GitHub mitcad/Mitcad#202 end to end in the Qt frontend
# (mitcad#1550): tools/ui-scripts/sketch3d-202.mitcad-ui, a rectangle
# sketched on a face, 3D Sketch checked, a line and a tangent arc onto a
# vertex of another component, a Pipe along them; the pipe's volume and
# ends and the saved design checked as tools/ui-sketch3d-202-test.sh does.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch3d-202
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch3d-202-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch3d-202")
  set_tests_properties(app.ui-macos-sketch3d-202 PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
