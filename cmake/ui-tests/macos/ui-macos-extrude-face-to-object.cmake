# SPDX-License-Identifier: MIT
# Extrude of a body face up to another face of the same body (mitcad#2033):
# the shared steps of tools/ui-extrude-face-to-object-test.sh in the Qt
# frontend, the lower step's top extruded To Object onto the higher step's
# top and joined, one level body.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-extrude-face-to-object
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-extrude-face-to-object-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-extrude-face-to-object")
  set_tests_properties(app.ui-macos-extrude-face-to-object PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
