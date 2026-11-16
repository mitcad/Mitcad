# SPDX-License-Identifier: MIT
# Dimension to a pipe bend's curved outline and a section's label dragged
# (mitcad#1909): the shared steps of tools/ui-drawing-curved-outline-test.sh
# in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-drawing-curved-outline
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drawing-curved-outline-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-drawing-curved-outline")
  set_tests_properties(app.ui-macos-drawing-curved-outline PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
