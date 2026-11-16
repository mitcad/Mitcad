# SPDX-License-Identifier: MIT
# Timber's Along an edge (mitcad#1814) in the Qt frontend: the shared steps
# of tools/ui-timber-edge-test.sh (tools/ui-scripts/timber-edge.mitcad-ui), a
# member along a plate's edge, flush, moved by its four offsets, following
# the plate when it gets shorter; a round edge refused in words. The timber
# library of tools/ui-timber-lib.sh, fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-timber-edge
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-timber-edge-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-timber-edge")
  set_tests_properties(app.ui-macos-timber-edge PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
