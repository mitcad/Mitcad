# SPDX-License-Identifier: MIT
# Fillet's Full Round (mitcad#2279): the shared steps of
# tools/ui-fillet-full-round-test.sh in the Qt frontend: Full Round hides
# Radius, a face picked into Edges is refused with a message saying to pick
# the face's two edges, and its two edges build the full round.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-fillet-full-round
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-fillet-full-round-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-fillet-full-round")
  set_tests_properties(app.ui-macos-fillet-full-round PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
