# SPDX-License-Identifier: MIT
# Sketch Dimension's right-click menu (mitcad#1540) and Make Driven / Make
# Driving on an existing dimension (mitcad#2055): the shared steps of
# tools/ui-sketch-tangent-dimension-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sketch-tangent-dimension
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sketch-tangent-dimension-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sketch-tangent-dimension")
  set_tests_properties(app.ui-macos-sketch-tangent-dimension PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
