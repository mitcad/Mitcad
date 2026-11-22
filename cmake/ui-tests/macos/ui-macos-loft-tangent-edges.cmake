# SPDX-License-Identifier: MIT
# The Loft panel's Tangent Edges (mitcad#2162): Merge by default, a circle
# to a slot gives one side face: the shared steps of
# tools/ui-loft-tangent-edges-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-loft-tangent-edges
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-loft-tangent-edges-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-loft-tangent-edges")
  set_tests_properties(app.ui-macos-loft-tangent-edges PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
