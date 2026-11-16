# SPDX-License-Identifier: MIT
# A new sphere's see-through preview without a pattern of triangles
# (mitcad#1575, GitHub mitcad/Mitcad#219): the shared steps of
# tools/ui-preview-transparency-test.sh in the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-preview-transparency
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-preview-transparency-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-preview-transparency")
  set_tests_properties(app.ui-macos-preview-transparency PROPERTIES
    TIMEOUT 180 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
